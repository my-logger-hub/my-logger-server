// The `TryFrom<HttpInputValue>` impl `MyHttpStringEnum` emits spells its error type `Self::Error`,
// which collides with our `Error` case. In type position it still resolves to the associated type,
// so the lint is the only fallout.
#![allow(ambiguous_associated_items)]

use my_http_server::macros::MyHttpStringEnum;
use my_logger::LogLevel;

use crate::repo::dto::LogLevelDto;

// `MyHttpStringEnum` now emits `Serialize`/`Deserialize` itself, over the `http_enum_case` value,
// so deriving serde here again would both conflict and key off the variant name instead.
#[derive(MyHttpStringEnum, Debug)]
pub enum LogLevelHttpModel {
    #[http_enum_case(id:0, description = "Info level")]
    Info,
    #[http_enum_case(id:1,description = "Warning level")]
    Warning,
    #[http_enum_case(id:2,description = "Error level")]
    Error,
    #[http_enum_case(id:3,description = "FatalError level")]
    FatalError,
    #[http_enum_case(id:4, description = "Debug level")]
    Debug,
}

impl Into<LogLevelDto> for LogLevelHttpModel {
    fn into(self) -> LogLevelDto {
        match self {
            LogLevelHttpModel::Info => LogLevelDto::Info,
            LogLevelHttpModel::Warning => LogLevelDto::Warning,
            LogLevelHttpModel::Error => LogLevelDto::Error,
            LogLevelHttpModel::FatalError => LogLevelDto::FatalError,
            LogLevelHttpModel::Debug => LogLevelDto::Debug,
        }
    }
}

impl Into<LogLevel> for LogLevelHttpModel {
    fn into(self) -> LogLevel {
        match self {
            LogLevelHttpModel::Info => LogLevel::Info,
            LogLevelHttpModel::Warning => LogLevel::Warning,
            LogLevelHttpModel::Error => LogLevel::Error,
            LogLevelHttpModel::FatalError => LogLevel::FatalError,
            LogLevelHttpModel::Debug => LogLevel::Debug,
        }
    }
}
