use std::collections::BTreeMap;

use my_logger::LogLevel;

use crate::log_item::LogEvent;

#[derive(Debug, Clone)]
pub struct IgnoreSingleEventModel {
    pub id: String,
    pub levels: Vec<LogLevel>,
    pub message_match: String,
    pub ctx_match: BTreeMap<String, String>,
    pub skip_amount: usize,
    pub minutes_to_wait: i64,
}

impl IgnoreSingleEventModel {
    fn has_level(&self, the_level: LogLevel) -> bool {
        for level in self.levels.iter() {
            if level.eq_to(&the_level) {
                return true;
            }
        }

        false
    }

    fn ctx_matches(&self, log_event: &LogEvent) -> bool {
        for (key, value) in log_event.ctx.iter() {
            match self.ctx_match.get(key) {
                Some(ctx_value) => {
                    if ctx_value != value {
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }

    pub fn matches_to(&self, log_event: &LogEvent) -> bool {
        if self.has_level(log_event.level) {
            return false;
        }

        if !log_event.message.contains(self.message_match.as_str()) {
            return false;
        }

        self.ctx_matches(log_event)
    }
}
