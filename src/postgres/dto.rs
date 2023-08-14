use my_logger::LogLevel;
use my_postgres::GroupByCount;
use my_postgres_macros::{
    DbEnumAsString, InsertDbEntity, SelectDbEntity, TableSchema, WhereDbModel,
};
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::app::LogCtxItem;
#[derive(DbEnumAsString)]
pub enum LogLevelDto {
    Info,
    Warning,
    Error,
    FatalError,
    Debug,
}

impl Into<LogLevelDto> for LogLevel {
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

#[derive(TableSchema, InsertDbEntity, SelectDbEntity)]
pub struct LogItemDto {
    #[primary_key(0)]
    pub tenant: String,
    #[primary_key(1)]
    #[sql_type("timestamp")]
    pub moment: DateTimeAsMicroseconds,
    pub id: String,
    pub process: String,
    pub level: LogLevelDto,
    pub message: String,
    #[sql_type("bjson")]
    pub context: Vec<LogCtxItem>,
}

#[derive(WhereDbModel)]
pub struct WhereModel<'s> {
    pub tenant: &'s str,
    #[sql_type("timestamp")]
    #[db_field_name("moment")]
    #[operator(">=")]
    pub from_date: DateTimeAsMicroseconds,
    #[sql_type("timestamp")]
    #[ignore_if_none]
    #[db_field_name("moment")]
    #[operator("<=")]
    pub to_date: Option<DateTimeAsMicroseconds>,
    #[ignore_if_none]
    pub level: Option<Vec<LogLevelDto>>,
}

#[derive(SelectDbEntity)]
pub struct StatisticsModel {
    #[group_by]
    pub level: LogLevelDto,
    pub count: GroupByCount,
}
