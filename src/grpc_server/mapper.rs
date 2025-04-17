use std::collections::BTreeMap;

use rust_extensions::SortableId;

use crate::{
    app::PROCESS_CONTEXT_KEY,
    my_logger_grpc::*,
    repo::dto::{IgnoreItemDto, LogItemDto},
};

impl Into<crate::app::LogItem> for LogEventGrpcModel {
    fn into(self) -> crate::app::LogItem {
        let level = self.level().into();

        let mut ctx = BTreeMap::new();

        for item in self.ctx {
            ctx.insert(item.key, item.value);
        }

        crate::app::LogItem {
            id: SortableId::generate().into(),
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

impl Into<crate::repo::dto::LogLevelDto> for LogLevelGrpcModel {
    fn into(self) -> crate::repo::dto::LogLevelDto {
        match self {
            LogLevelGrpcModel::Info => crate::repo::dto::LogLevelDto::Info,
            LogLevelGrpcModel::Warning => crate::repo::dto::LogLevelDto::Warning,
            LogLevelGrpcModel::Error => crate::repo::dto::LogLevelDto::Error,
            LogLevelGrpcModel::Fatal => crate::repo::dto::LogLevelDto::FatalError,
            LogLevelGrpcModel::Debug => crate::repo::dto::LogLevelDto::Debug,
        }
    }
}

impl Into<LogLevelGrpcModel> for crate::repo::dto::LogLevelDto {
    fn into(self) -> LogLevelGrpcModel {
        match self {
            crate::repo::dto::LogLevelDto::Info => LogLevelGrpcModel::Info,
            crate::repo::dto::LogLevelDto::Warning => LogLevelGrpcModel::Warning,
            crate::repo::dto::LogLevelDto::Error => LogLevelGrpcModel::Error,
            crate::repo::dto::LogLevelDto::FatalError => LogLevelGrpcModel::Fatal,
            crate::repo::dto::LogLevelDto::Debug => LogLevelGrpcModel::Debug,
        }
    }
}

pub fn to_log_event_grpc_model(mut src: LogItemDto) -> LogEventGrpcModel {
    let log_level_grpc: LogLevelGrpcModel = src.level.into();

    let process_name = src.context.remove(PROCESS_CONTEXT_KEY);

    LogEventGrpcModel {
        tenant_id: String::new(),
        timestamp: src.moment.unix_microseconds,
        process_name: process_name.unwrap_or_default(),
        message: src.message,
        level: log_level_grpc as i32,
        ctx: src
            .context
            .into_iter()
            .map(|(key, value)| LogEventContext { key, value })
            .collect(),
    }
}

impl Into<IgnoreItemDto> for IgnoreEventGrpcModel {
    fn into(self) -> IgnoreItemDto {
        IgnoreItemDto {
            level: self.level().into(),
            application: self.application,
            marker: self.marker,
        }
    }
}

impl Into<IgnoreEventGrpcModel> for IgnoreItemDto {
    fn into(self) -> IgnoreEventGrpcModel {
        let log_level_grpc: LogLevelGrpcModel = self.level.into();
        IgnoreEventGrpcModel {
            application: self.application,
            marker: self.marker,
            level: log_level_grpc as i32,
        }
    }
}
