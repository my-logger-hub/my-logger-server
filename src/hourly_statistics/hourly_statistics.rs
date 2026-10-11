use std::collections::BTreeMap;

use my_logger::LogLevel;
use serde::{Deserialize, Serialize};

use crate::app::LogItem;

use super::StatisticsHour;

pub const MAX_HOURS_TO_KEEP: usize = 48;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct HourlyStatisticsItem {
    pub info: u32,
    pub warning: u32,
    pub error: u32,
    pub fatal_error: u32,
    pub debug: u32,
}

impl HourlyStatisticsItem {
    fn add(&mut self, other: &HourlyStatisticsItem) {
        self.info += other.info;
        self.warning += other.warning;
        self.error += other.error;
        self.fatal_error += other.fatal_error;
        self.debug += other.debug;
    }
}

/// Product scope → application → counters
pub type ScopesStatistics = BTreeMap<String, BTreeMap<String, HourlyStatisticsItem>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedHour {
    pub hour: u64,
    pub scopes: ScopesStatistics,
}

/// The counters of every product scope, by hour, scope and application
pub struct HourlyStatistics {
    data: BTreeMap<StatisticsHour, ScopesStatistics>,
}

impl HourlyStatistics {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn restore_from_vec(&mut self, items: Vec<PersistedHour>) {
        for entry in items {
            if entry.scopes.is_empty() {
                continue;
            }
            let hour: StatisticsHour = entry.hour.into();
            self.data.insert(hour, entry.scopes);
        }
    }

    pub fn snapshot(&self) -> Vec<PersistedHour> {
        self.data
            .iter()
            .map(|(hour, scopes)| PersistedHour {
                hour: hour.get_value(),
                scopes: scopes.clone(),
            })
            .collect()
    }

    /// `scope` is the product scope the record went to
    pub fn update(&mut self, log_item: &LogItem, scope: &str) {
        let app = match log_item.ctx.get("Application") {
            Some(a) => a,
            None => return,
        };

        let key: StatisticsHour = log_item.timestamp.into();
        let by_app = self
            .data
            .entry(key)
            .or_default()
            .entry(scope.to_string())
            .or_default()
            .entry(app.to_string())
            .or_insert_with(HourlyStatisticsItem::default);

        match log_item.level {
            LogLevel::Info => by_app.info += 1,
            LogLevel::Warning => by_app.warning += 1,
            LogLevel::Error => by_app.error += 1,
            LogLevel::FatalError => by_app.fatal_error += 1,
            LogLevel::Debug => by_app.debug += 1,
        }
    }

    pub fn get_max_hours(&self, max_hours: usize) -> Vec<(StatisticsHour, ScopesStatistics)> {
        let mut day_no = 0;

        let mut result = Vec::new();
        for (key, items) in self.data.iter().rev() {
            if day_no >= max_hours {
                break;
            }

            result.push((key.clone(), items.clone()));

            day_no += 1;
        }

        result
    }

    /// The counters of every application of every scope of the hour, added up
    pub fn get_hour_totals(&self, hour: StatisticsHour) -> HourlyStatisticsItem {
        let mut result = HourlyStatisticsItem::default();

        for item in self
            .data
            .get(&hour)
            .into_iter()
            .flat_map(|scopes| scopes.values())
            .flat_map(|apps| apps.values())
        {
            result.add(item);
        }

        result
    }

    pub fn hours_count(&self) -> usize {
        self.data.len()
    }

    pub fn gc(&mut self) {
        while self.data.len() > MAX_HOURS_TO_KEEP {
            let key = self.data.keys().next().unwrap().clone();
            self.data.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_extensions::date_time::DateTimeAsMicroseconds;

    use super::*;

    fn log_item(application: &str, level: LogLevel) -> LogItem {
        LogItem {
            id: String::new(),
            level,
            process: None,
            message: String::new(),
            timestamp: DateTimeAsMicroseconds::now(),
            ctx: BTreeMap::from([("Application".to_string(), application.to_string())]),
        }
    }

    #[test]
    fn an_application_is_counted_in_the_scope_its_records_went_to() {
        let mut statistics = HourlyStatistics::new();

        statistics.update(&log_item("quotes", LogLevel::Error), "trading");
        statistics.update(&log_item("quotes", LogLevel::Error), "trading");
        statistics.update(&log_item("quotes", LogLevel::Warning), "by-header");
        statistics.update(&log_item("billing", LogLevel::Info), "default");

        let hours = statistics.get_max_hours(1);
        let (hour, scopes) = &hours[0];

        assert_eq!(scopes["trading"]["quotes"].error, 2);
        assert_eq!(scopes["by-header"]["quotes"].warning, 1);
        assert_eq!(scopes["default"]["billing"].info, 1);

        let totals = statistics.get_hour_totals(*hour);
        assert_eq!(totals.error, 2);
        assert_eq!(totals.warning, 1);
        assert_eq!(totals.info, 1);
    }

    #[test]
    fn the_statistics_are_restored_as_they_are_persisted() {
        let mut statistics = HourlyStatistics::new();
        statistics.update(&log_item("quotes", LogLevel::Error), "trading");
        statistics.update(&log_item("billing", LogLevel::Info), "default");

        let json = serde_json::to_string(&statistics.snapshot()).unwrap();

        let mut restored = HourlyStatistics::new();
        restored.restore_from_vec(serde_json::from_str(&json).unwrap());

        let snapshot = restored.snapshot();
        assert_eq!(snapshot[0].scopes["trading"]["quotes"].error, 1);
        assert_eq!(snapshot[0].scopes["default"]["billing"].info, 1);
    }
}
