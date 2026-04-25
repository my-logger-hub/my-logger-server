use std::collections::BTreeMap;

use my_logger::LogLevel;
use my_sqlite::macros::*;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::app::LogItem;

#[derive(DbEnumAsString, Debug, Clone, PartialEq, Eq)]
pub enum LogLevelDto {
    Info,
    Warning,
    Error,
    FatalError,
    Debug,
}

impl LogLevelDto {
    pub fn is_info(&self) -> bool {
        match self {
            LogLevelDto::Info => true,
            _ => false,
        }
    }

    pub fn is_warning(&self) -> bool {
        match self {
            LogLevelDto::Warning => true,
            _ => false,
        }
    }

    pub fn is_error(&self) -> bool {
        match self {
            LogLevelDto::Error => true,
            _ => false,
        }
    }

    pub fn is_fatal_error(&self) -> bool {
        match self {
            LogLevelDto::FatalError => true,
            _ => false,
        }
    }

    pub fn is_debug(&self) -> bool {
        match self {
            LogLevelDto::Debug => true,
            _ => false,
        }
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

#[derive(TableSchema, InsertDbEntity, SelectDbEntity, UpdateDbEntity, Debug)]
pub struct IgnoreItemDto {
    #[primary_key(0)]
    #[generate_where_model("IgnoreWhereModel")]
    pub level: LogLevelDto,
    #[primary_key(1)]
    #[generate_where_model("IgnoreWhereModel")]
    pub application: String,
    #[primary_key(2)]
    #[generate_where_model("IgnoreWhereModel")]
    pub marker: String,
}

impl IgnoreItemDto {
    pub fn matches_ignore_filter(&self, log_event: &LogItem) -> bool {
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
