use std::collections::BTreeMap;

use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::{Deserialize, Serialize};

use crate::app::LogItem;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevelDto {
    Info,
    Warning,
    Error,
    FatalError,
    Debug,
}

impl LogLevelDto {
    pub fn is_info(&self) -> bool {
        matches!(self, LogLevelDto::Info)
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, LogLevelDto::Warning)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, LogLevelDto::Error)
    }

    pub fn is_fatal_error(&self) -> bool {
        matches!(self, LogLevelDto::FatalError)
    }

    pub fn is_debug(&self) -> bool {
        matches!(self, LogLevelDto::Debug)
    }
}

impl<'s> Into<LogLevelDto> for &'s LogLevel {
    fn into(self) -> LogLevelDto {
        match self {
            LogLevel::Info => LogLevelDto::Info,
            LogLevel::Warning => LogLevelDto::Warning,
            LogLevel::Error => LogLevelDto::Error,
            LogLevel::FatalError => LogLevelDto::FatalError,
            LogLevel::Debug => LogLevelDto::Debug,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogItemDto {
    pub moment: DateTimeAsMicroseconds,
    pub id: String,
    pub level: LogLevelDto,
    pub message: String,
    pub context: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct StatisticsModel {
    pub level: LogLevelDto,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IgnoreItemDto {
    pub level: LogLevelDto,
    pub application: String,
    pub marker: String,
    /// Optional expiration moment as unix microseconds. `None` means the rule never expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl IgnoreItemDto {
    pub fn is_expired(&self, now: DateTimeAsMicroseconds) -> bool {
        match self.expires_at {
            Some(expires_at) => expires_at <= now.unix_microseconds,
            None => false,
        }
    }

    pub fn matches_ignore_filter(&self, log_event: &LogItem) -> bool {
        if self.is_expired(DateTimeAsMicroseconds::now()) {
            return false;
        }

        if !log_event.is_level(&self.level) {
            return false;
        }

        if !log_event.is_application(&self.application) {
            return false;
        }

        if self.marker.as_str() == "*" {
            return true;
        }

        log_event.has_entry(self.marker.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct IgnoreWhereModel {
    pub level: LogLevelDto,
    pub application: String,
    pub marker: String,
}

impl IgnoreWhereModel {
    pub fn matches(&self, item: &IgnoreItemDto) -> bool {
        item.level == self.level
            && item.application == self.application
            && item.marker == self.marker
    }
}
