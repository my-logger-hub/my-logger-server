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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedHour {
    pub hour: u64,
    pub apps: BTreeMap<String, HourlyStatisticsItem>,
}

pub struct HourlyStatistics {
    data: BTreeMap<StatisticsHour, BTreeMap<String, HourlyStatisticsItem>>,
}

impl HourlyStatistics {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn restore_from_vec(&mut self, items: Vec<PersistedHour>) {
        for entry in items {
            if entry.apps.is_empty() {
                continue;
            }
            let hour: StatisticsHour = entry.hour.into();
            self.data.insert(hour, entry.apps);
        }
    }

    pub fn snapshot(&self) -> Vec<PersistedHour> {
        self.data
            .iter()
            .map(|(hour, apps)| PersistedHour {
                hour: hour.get_value(),
                apps: apps.clone(),
            })
            .collect()
    }

    pub fn update(&mut self, log_item: &LogItem) {
        let app = match log_item.ctx.get("Application") {
            Some(a) => a,
            None => return,
        };

        let key: StatisticsHour = log_item.timestamp.into();
        let by_date = self.data.entry(key).or_insert_with(BTreeMap::new);
        let by_app = by_date
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

    pub fn get_max_hours(
        &self,
        max_hours: usize,
    ) -> Vec<(StatisticsHour, BTreeMap<String, HourlyStatisticsItem>)> {
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

    pub fn gc(&mut self) {
        while self.data.len() > MAX_HOURS_TO_KEEP {
            let key = self.data.keys().next().unwrap().clone();
            self.data.remove(&key);
        }
    }
}
