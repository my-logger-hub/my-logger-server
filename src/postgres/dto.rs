use std::collections::BTreeMap;

use my_logger::LogLevel;
use my_postgres::macros::*;
use my_postgres::GroupByCount;
use rust_extensions::date_time::DateTimeAsMicroseconds;

#[derive(DbEnumAsString, Debug)]
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

#[derive(TableSchema, InsertDbEntity, SelectDbEntity, Debug)]
pub struct LogItemDto {
    #[primary_key(0)]
    pub tenant: String,
    #[primary_key(1)]
    #[sql_type("timestamp")]
    #[order_by_desc]
    pub moment: DateTimeAsMicroseconds,
    #[db_index(id:0, index_name:"id_idx", is_unique:true, order:"ASC")]
    pub id: String,
    pub process: String,
    pub level: LogLevelDto,
    pub message: String,
    #[sql_type("bjson")]
    pub context: BTreeMap<String, String>,
}

#[derive(WhereDbModel)]
pub struct WhereModel<'s> {
    pub tenant: &'s str,
    #[sql_type("timestamp")]
    #[db_column_name("moment")]
    #[operator(">=")]
    pub from_date: DateTimeAsMicroseconds,
    #[sql_type("timestamp")]
    #[ignore_if_none]
    #[db_column_name("moment")]
    #[operator("<=")]
    pub to_date: Option<DateTimeAsMicroseconds>,
    #[ignore_if_none]
    pub level: Option<Vec<LogLevelDto>>,
    #[sql_type("bjson")]
    #[ignore_if_none]
    pub context: Option<BTreeMap<String, String>>,
    #[limit]
    pub take: usize,
}

#[derive(WhereDbModel)]
pub struct WhereStatisticsModel<'s> {
    pub tenant: &'s str,
    #[sql_type("timestamp")]
    #[db_column_name("moment")]
    #[operator(">=")]
    pub from_date: DateTimeAsMicroseconds,
    #[sql_type("timestamp")]
    #[ignore_if_none]
    #[db_column_name("moment")]
    #[operator("<=")]
    pub to_date: Option<DateTimeAsMicroseconds>,
    #[ignore_if_none]
    pub level: Option<Vec<LogLevelDto>>,
}

#[derive(SelectDbEntity)]
pub struct StatisticsModel {
    #[group_by]
    pub level: LogLevelDto,
    pub count: GroupByCount<i32>,
}
