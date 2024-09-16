use std::collections::BTreeMap;

use my_logger::LogLevel;

use crate::{app::LogItem, repo::HourStatisticsDto};

use super::StatisticsHour;

pub const MAX_HOURS_TO_KEEP: usize = 48;

#[derive(Debug, Clone, Copy, Default)]
pub struct HourlyStatisticsItem {
    pub info: u32,
    pub warning: u32,
    pub error: u32,
    pub fatal_error: u32,
    pub debug: u32,
}

pub struct HourlyStatistics {
    data: BTreeMap<StatisticsHour, BTreeMap<String, HourlyStatisticsItem>>,
    pub to_persist: BTreeMap<StatisticsHour, BTreeMap<String, HourlyStatisticsItem>>,
}

impl HourlyStatistics {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            to_persist: BTreeMap::new(),
        }
    }

    pub fn restore(&mut self, data: HourStatisticsDto) {
        let key = data.date_key.into();
        if !self.data.contains_key(&key) {
            self.data.insert(key, BTreeMap::new());
        }

        let by_date = self.data.get_mut(&key).unwrap();

        by_date.insert(
            data.app,
            HourlyStatisticsItem {
                info: data.info,
                warning: data.warning,
                error: data.error,
                fatal_error: data.fatal_error,
                debug: data.debug,
            },
        );
    }

    pub fn update(&mut self, log_item: &LogItem) {
        let (key, app, itm) = {
            let app = log_item.ctx.get("Application");

            if app.is_none() {
                return;
            }

            let app = app.unwrap();

            let key: StatisticsHour = log_item.timestamp.into();
            if !self.data.contains_key(&key) {
                self.data.insert(key, BTreeMap::new());
            }

            let by_date = self.data.get_mut(&key).unwrap();

            if !by_date.contains_key(app) {
                by_date.insert(app.to_string(), HourlyStatisticsItem::default());
            }

            let by_app = by_date.get_mut(app).unwrap();

            match log_item.level {
                LogLevel::Info => by_app.info += 1,
                LogLevel::Warning => by_app.warning += 1,
                LogLevel::Error => by_app.error += 1,
                LogLevel::FatalError => by_app.fatal_error += 1,
                LogLevel::Debug => by_app.debug += 1,
            }

            (key, app.to_string(), by_app.clone())
        };

        self.set_to_persist(key, app, itm)
    }

    fn set_to_persist(&mut self, key: StatisticsHour, app: String, itm: HourlyStatisticsItem) {
        if !self.to_persist.contains_key(&key) {
            self.to_persist.insert(key, BTreeMap::new());
        }

        let by_date = self.to_persist.get_mut(&key).unwrap();

        by_date.insert(app, itm);
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
