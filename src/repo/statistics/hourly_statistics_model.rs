use my_logger::LogLevel;

use crate::log_item::LogEvent;

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct HourlyStatisticsModel {
    pub info: u32,
    pub warning: u32,
    pub error: u32,
    pub fatal_error: u32,
    pub debug: u32,
}

impl HourlyStatisticsModel {
    pub fn from_log_item(src: &LogEvent) -> Self {
        let mut result = Self::default();
        result.inc(src.level);
        result
    }

    pub fn inc(&mut self, level: LogLevel) {
        match level {
            my_logger::LogLevel::Info => self.info += 1,
            my_logger::LogLevel::Warning => self.warning += 1,
            my_logger::LogLevel::Error => self.error += 1,
            my_logger::LogLevel::FatalError => self.fatal_error += 1,
            my_logger::LogLevel::Debug => self.debug += 1,
        }
    }
}
