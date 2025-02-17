use my_http_server::macros::MyHttpStringEnum;
use my_logger::LogLevel;
use serde::Deserialize;

#[derive(MyHttpStringEnum, Deserialize, Debug, Clone, Copy)]
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
