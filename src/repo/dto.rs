use std::collections::BTreeMap;

use my_logger::LogLevel;
use my_sqlite::macros::*;
use my_sqlite::GroupByCount;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::app::LogItem;

#[derive(DbEnumAsString, Debug, Clone)]
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
    #[sql_type("timestamp")]
    #[order_by_desc]
    pub moment: DateTimeAsMicroseconds,
    #[db_index(id:0, index_name:"id_idx", is_unique:true, order:"ASC")]
    pub id: String,
    pub level: LogLevelDto,
    pub message: String,
    #[sql_type("jsonb")]
    pub context: BTreeMap<String, String>,
}

#[derive(WhereDbModel)]
pub struct WhereModel {
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
    #[sql_type("jsonb")]
    #[ignore_if_none]
    pub context: Option<BTreeMap<String, String>>,
    #[limit]
    pub take: usize,
}

#[derive(WhereDbModel)]
pub struct ScanWhereModel {
    #[sql_type("timestamp")]
    #[db_column_name("moment")]
    #[operator(">=")]
    pub from_date: DateTimeAsMicroseconds,

    #[sql_type("timestamp")]
    #[db_column_name("moment")]
    #[operator(">=")]
    pub to_date: DateTimeAsMicroseconds,

    #[limit]
    pub take: usize,
}

#[where_raw_model("moment => ${from_date} AND moment <= ${to_date} AND (message LIKE '%' || ${phrase} || '%' OR context LIKE '%' || ${phrase} || '%')")]
pub struct WhereScanModel<'s> {
    pub from_date: i64,
    pub to_date: i64,
    pub phrase: &'s str,
    pub limit: usize,
}

#[derive(WhereDbModel)]
pub struct DeleteLevelWhereModel {
    #[operator("<=")]
    #[sql_type("timestamp")]
    pub moment: DateTimeAsMicroseconds,

    pub level: LogLevelDto,
}

#[derive(WhereDbModel)]
pub struct WhereStatisticsModel {
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
