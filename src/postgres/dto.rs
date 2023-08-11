use my_postgres_macros::{InsertDbEntity, SelectDbEntity, TableSchema};
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::app::LogCtxItem;

#[derive(TableSchema, InsertDbEntity, SelectDbEntity)]
pub struct LogItemDto {
    #[primary_key(0)]
    pub tenant: String,
    #[primary_key(1)]
    #[sql_type("timestamp")]
    pub moment: DateTimeAsMicroseconds,
    pub id: String,
    pub process: String,
    pub level: String,
    pub message: String,
    #[sql_type("bjson")]
    pub context: Vec<LogCtxItem>,
}
