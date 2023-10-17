use crate::{app::LogCtxItem, my_logger_grpc::*, postgres::dto::LogItemDto};

impl Into<crate::app::LogItem> for LogEventGrpcModel {
    fn into(self) -> crate::app::LogItem {
        let level = self.level().into();
        crate::app::LogItem {
            id: crate::utils::generate_log_id(),
            tenant: self.tenant_id,
            level,
            process: self.process_name.into(),
            message: self.message,
            timestamp: self.timestamp.into(),
            ctx: self
                .ctx
                .into_iter()
                .map(|item| crate::app::LogCtxItem {
                    key: item.key,
                    value: item.value,
                })
                .collect(),
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

impl Into<crate::postgres::dto::LogLevelDto> for LogLevelGrpcModel {
    fn into(self) -> crate::postgres::dto::LogLevelDto {
        match self {
            LogLevelGrpcModel::Info => crate::postgres::dto::LogLevelDto::Info,
            LogLevelGrpcModel::Warning => crate::postgres::dto::LogLevelDto::Warning,
            LogLevelGrpcModel::Error => crate::postgres::dto::LogLevelDto::Error,
            LogLevelGrpcModel::Fatal => crate::postgres::dto::LogLevelDto::FatalError,
            LogLevelGrpcModel::Debug => crate::postgres::dto::LogLevelDto::Debug,
        }
    }
}

impl Into<LogLevelGrpcModel> for crate::postgres::dto::LogLevelDto {
    fn into(self) -> LogLevelGrpcModel {
        match self {
            crate::postgres::dto::LogLevelDto::Info => LogLevelGrpcModel::Info,
            crate::postgres::dto::LogLevelDto::Warning => LogLevelGrpcModel::Warning,
            crate::postgres::dto::LogLevelDto::Error => LogLevelGrpcModel::Error,
            crate::postgres::dto::LogLevelDto::FatalError => LogLevelGrpcModel::Fatal,
            crate::postgres::dto::LogLevelDto::Debug => LogLevelGrpcModel::Debug,
        }
    }
}

impl Into<LogEventContext> for LogCtxItem {
    fn into(self) -> LogEventContext {
        LogEventContext {
            key: self.key,
            value: self.value,
        }
    }
}

impl Into<LogEventGrpcModel> for LogItemDto {
    fn into(self) -> LogEventGrpcModel {
        let log_level_grpc: LogLevelGrpcModel = self.level.into();
        LogEventGrpcModel {
            tenant_id: self.tenant,
            timestamp: self.moment.unix_microseconds,
            process_name: self.process,
            message: self.message,
            level: log_level_grpc as i32,
            ctx: self.context.into_iter().map(|item| item.into()).collect(),
        }
    }
}