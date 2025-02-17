use std::collections::BTreeMap;

use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{
    app::APPLICATION_KEY,
    repo::logs::{LogEventCtxFileGrpcModel, LogEventFileGrpcModel},
};

#[derive(Debug)]
pub struct LogEvent {
    pub id: String,
    pub level: LogLevel,
    pub process: Option<String>,
    pub message: String,
    pub timestamp: DateTimeAsMicroseconds,
    pub ctx: BTreeMap<String, String>,
}

impl LogEvent {
    pub fn is_application(&self, application: &str) -> bool {
        if let Some(app_name) = self.ctx.get(APPLICATION_KEY) {
            return app_name == application;
        }

        false
    }

    pub fn get_application(&self) -> Option<&str> {
        if let Some(app_name) = self.ctx.get(APPLICATION_KEY) {
            return Some(app_name.as_str());
        }

        None
    }

    pub fn has_entry(&self, entry: &str) -> bool {
        if let Some(process) = &self.process {
            return process.contains(entry) || self.message.contains(entry);
        }

        self.message.contains(entry)
    }

    /*
    pub fn is_level(&self, level: &LogLevelDto) -> bool {
        match level {
            LogLevelDto::Info => level.is_info(),
            LogLevelDto::Warning => level.is_warning(),
            LogLevelDto::Error => level.is_error(),
            LogLevelDto::FatalError => level.is_fatal_error(),
            LogLevelDto::Debug => level.is_debug(),
        }
    }
     */
}

impl Into<LogEventFileGrpcModel> for &'_ LogEvent {
    fn into(self) -> LogEventFileGrpcModel {
        LogEventFileGrpcModel {
            id: self.id.to_string(),
            timestamp: self.timestamp.unix_microseconds,
            level: self.level.to_u8() as i32,
            message: self.message.to_string(),
            ctx: self
                .ctx
                .iter()
                .map(|(key, value)| LogEventCtxFileGrpcModel {
                    key: key.to_string(),
                    value: value.to_string(),
                })
                .collect(),
            process: self.process.clone(),
        }
    }
}
