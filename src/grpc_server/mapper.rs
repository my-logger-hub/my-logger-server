use std::collections::BTreeMap;

use crate::{my_logger_grpc::*, repo::logs::LogEventFileGrpcModel};

impl Into<crate::log_item::LogEvent> for LogEventGrpcModel {
    fn into(self) -> crate::log_item::LogEvent {
        let level = self.level().into();

        let mut ctx = BTreeMap::new();

        for item in self.ctx {
            ctx.insert(item.key, item.value);
        }

        crate::log_item::LogEvent {
            id: crate::utils::generate_log_id(),
            level,

            process: self.process_name.into(),
            message: self.message,
            timestamp: self.timestamp.into(),
            ctx,
        }
    }
}

impl Into<my_logger::LogLevel> for LogLevelGrpcModel {
    fn into(self) -> my_logger::LogLevel {
        match self {
            LogLevelGrpcModel::Info => my_logger::LogLevel::Info,
            LogLevelGrpcModel::Warning => my_logger::LogLevel::Warning,
            LogLevelGrpcModel::Error => my_logger::LogLevel::Error,
            LogLevelGrpcModel::Fatal => my_logger::LogLevel::FatalError,
            LogLevelGrpcModel::Debug => my_logger::LogLevel::Debug,
        }
    }
}

impl Into<LogLevelGrpcModel> for my_logger::LogLevel {
    fn into(self) -> LogLevelGrpcModel {
        match self {
            my_logger::LogLevel::Info => LogLevelGrpcModel::Info,
            my_logger::LogLevel::Warning => LogLevelGrpcModel::Warning,
            my_logger::LogLevel::Error => LogLevelGrpcModel::Error,
            my_logger::LogLevel::FatalError => LogLevelGrpcModel::Fatal,
            my_logger::LogLevel::Debug => LogLevelGrpcModel::Debug,
        }
    }
}

impl Into<LogLevelGrpcModel> for &'_ my_logger::LogLevel {
    fn into(self) -> LogLevelGrpcModel {
        match self {
            my_logger::LogLevel::Info => LogLevelGrpcModel::Info,
            my_logger::LogLevel::Warning => LogLevelGrpcModel::Warning,
            my_logger::LogLevel::Error => LogLevelGrpcModel::Error,
            my_logger::LogLevel::FatalError => LogLevelGrpcModel::Fatal,
            my_logger::LogLevel::Debug => LogLevelGrpcModel::Debug,
        }
    }
}

pub fn to_log_event_grpc_model(src: LogEventFileGrpcModel) -> LogEventGrpcModel {
    let log_level: LogLevelGrpcModel = src.get_log_level().into();

    LogEventGrpcModel {
        tenant_id: String::new(),
        timestamp: src.timestamp,
        process_name: src.process.unwrap_or_default(),
        message: src.message,
        level: log_level.into(),
        ctx: src
            .ctx
            .into_iter()
            .map(|itm| LogEventContext {
                key: itm.key,
                value: itm.value,
            })
            .collect(),
    }
}

impl Into<crate::repo::ignore_events::IgnoreEventModel> for IgnoreEventGrpcModel {
    fn into(self) -> crate::repo::ignore_events::IgnoreEventModel {
        crate::repo::ignore_events::IgnoreEventModel {
            level: self.level().into(),
            application: self.application,
            marker: self.marker,
        }
    }
}

impl Into<IgnoreEventGrpcModel> for crate::repo::ignore_events::IgnoreEventModel {
    fn into(self) -> IgnoreEventGrpcModel {
        let log_level_grpc: LogLevelGrpcModel = self.level.into();
        IgnoreEventGrpcModel {
            application: self.application,
            marker: self.marker,
            level: log_level_grpc.into(),
        }
    }
}

impl Into<crate::repo::ignore_single_events::IgnoreSingleEventModel>
    for IgnoreSingleEventGrpcModel
{
    fn into(self) -> crate::repo::ignore_single_events::IgnoreSingleEventModel {
        crate::repo::ignore_single_events::IgnoreSingleEventModel {
            levels: self.levels().into_iter().map(|itm| itm.into()).collect(),
            id: self.id,
            message_match: self.message_match,
            ctx_match: self
                .context_match
                .into_iter()
                .map(|itm| (itm.key, itm.value))
                .collect(),
            skip_amount: self.skip_amount as usize,
            minutes_to_wait: self.minutes_to_wait as i64,
        }
    }
}

impl Into<IgnoreSingleEventGrpcModel>
    for &'_ crate::repo::ignore_single_events::IgnoreSingleEventModel
{
    fn into(self) -> IgnoreSingleEventGrpcModel {
        IgnoreSingleEventGrpcModel {
            id: self.id.to_string(),

            message_match: self.message_match.to_string(),

            skip_amount: self.skip_amount as u64,
            minutes_to_wait: self.skip_amount as u64,

            levels: self
                .levels
                .iter()
                .map(|itm| {
                    let log_level: LogLevelGrpcModel = itm.into();

                    log_level.into()
                })
                .collect(),
            context_match: self
                .ctx_match
                .iter()
                .map(|(key, value)| LogEventContext {
                    key: key.to_string(),
                    value: value.to_string(),
                })
                .collect(),
        }
    }
}
