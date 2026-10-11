use std::collections::BTreeMap;

use crate::{
    app::{LogItem, PROCESS_CONTEXT_KEY},
    repo::dto::LogLevelDto,
};

use super::{level_value, LogRecordProto, LogsStream, LEVEL_KEY};

/// What a search asks of a record: one of the levels, every context pair, and the phrase
/// somewhere in its message, process or context values. Both are matched ignoring case.
pub struct SearchFilter {
    levels: Option<Vec<LogLevelDto>>,
    /// Lowercase
    context: Vec<(String, String)>,
    /// Lowercase
    phrase: Option<String>,
}

impl SearchFilter {
    pub fn new(
        levels: Option<Vec<LogLevelDto>>,
        context: Option<BTreeMap<String, String>>,
        phrase: Option<&str>,
    ) -> Self {
        Self {
            levels: levels.filter(|levels| !levels.is_empty()),
            context: context
                .unwrap_or_default()
                .into_iter()
                .map(|(key, value)| (key.to_lowercase(), value.to_lowercase()))
                .collect(),
            phrase: phrase
                .map(str::trim)
                .filter(|phrase| !phrase.is_empty())
                .map(str::to_lowercase),
        }
    }

    /// Whether a level of the stream is asked for
    pub fn wants(&self, stream: LogsStream) -> bool {
        match &self.levels {
            None => true,
            Some(levels) => levels.iter().any(|level| LogsStream::of(level) == stream),
        }
    }

    /// What the filter asks of the keys a key index counts: each condition is met by a
    /// record with any of its `key=value`s
    pub fn conditions(&self) -> Vec<Vec<(String, String)>> {
        let mut result = Vec::new();

        if let Some(levels) = &self.levels {
            result.push(
                levels
                    .iter()
                    .map(|level| (LEVEL_KEY.to_string(), level_value(level).to_string()))
                    .collect(),
            );
        }

        for (key, value) in &self.context {
            result.push(vec![(key.clone(), value.clone())]);
        }

        result
    }

    pub fn matches_record(&self, record: &LogRecordProto) -> bool {
        self.matches(
            record.get_level(),
            record.process.as_deref(),
            &record.message,
            record
                .ctx
                .iter()
                .map(|ctx| (ctx.key.as_str(), ctx.value.as_str())),
        )
    }

    pub fn matches_item(&self, item: &LogItem) -> bool {
        self.matches(
            (&item.level).into(),
            item.process.as_deref(),
            &item.message,
            item.ctx
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
    }

    fn matches<'s>(
        &self,
        level: LogLevelDto,
        process: Option<&'s str>,
        message: &'s str,
        ctx: impl Iterator<Item = (&'s str, &'s str)> + Clone,
    ) -> bool {
        if let Some(levels) = &self.levels {
            if !levels.contains(&level) {
                return false;
            }
        }

        for (key, value) in &self.context {
            let is_process = key.eq_ignore_ascii_case(PROCESS_CONTEXT_KEY)
                && process.map_or(false, |process| eq_ignore_case(process, value));

            if !is_process
                && !ctx
                    .clone()
                    .any(|(ctx_key, ctx_value)| {
                        eq_ignore_case(ctx_key, key) && eq_ignore_case(ctx_value, value)
                    })
            {
                return false;
            }
        }

        match &self.phrase {
            None => true,
            Some(phrase) => {
                contains_ignore_case(message, phrase)
                    || process.map_or(false, |process| contains_ignore_case(process, phrase))
                    || ctx.clone().any(|(_, value)| contains_ignore_case(value, phrase))
            }
        }
    }
}

fn eq_ignore_case(value: &str, lowercase: &str) -> bool {
    if value.is_ascii() && lowercase.is_ascii() {
        value.eq_ignore_ascii_case(lowercase)
    } else {
        value.to_lowercase() == lowercase
    }
}

fn contains_ignore_case(value: &str, lowercase: &str) -> bool {
    if lowercase.is_ascii() {
        let needle = lowercase.as_bytes();

        value
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
    } else {
        value.to_lowercase().contains(lowercase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs_storage::LogRecordCtxProto;

    fn record() -> LogRecordProto {
        LogRecordProto {
            timestamp: 1,
            level: 2,
            process: Some("PaymentsProcessor".to_string()),
            message: "Connection Timeout while calling the bank".to_string(),
            ctx: vec![LogRecordCtxProto {
                key: "Application".to_string(),
                value: "Billing".to_string(),
            }],
        }
    }

    #[test]
    fn matches_ignoring_case() {
        let ctx = BTreeMap::from([("application".to_string(), "BILLING".to_string())]);
        let filter = SearchFilter::new(Some(vec![LogLevelDto::Error]), Some(ctx), Some("timeout"));
        assert!(filter.matches_record(&record()));

        let process = BTreeMap::from([("Process".to_string(), "paymentsprocessor".to_string())]);
        assert!(SearchFilter::new(None, Some(process), None).matches_record(&record()));

        assert!(SearchFilter::new(None, None, Some("payments")).matches_record(&record()));
        assert!(SearchFilter::new(None, None, Some("billing")).matches_record(&record()));
    }

    #[test]
    fn does_not_match() {
        let filter = SearchFilter::new(Some(vec![LogLevelDto::Warning]), None, None);
        assert!(!filter.matches_record(&record()));

        let ctx = BTreeMap::from([("Application".to_string(), "Trading".to_string())]);
        assert!(!SearchFilter::new(None, Some(ctx), None).matches_record(&record()));

        assert!(!SearchFilter::new(None, None, Some("refused")).matches_record(&record()));
    }
}
