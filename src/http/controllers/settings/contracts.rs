use my_http_server::macros::{MyHttpInput, MyHttpObjectStructure};
use serde::Serialize;

use crate::http::controllers::shared_contract::LogLevelHttpModel;

#[derive(Debug, MyHttpInput)]
pub struct PostIgnoreMaskHttpInput {
    #[http_body(description: "Log Level")]
    pub level: LogLevelHttpModel,

    #[http_body(description: "Application name")]
    pub application: String,

    #[http_body(description: "Filter marker")]
    pub marker: String,

    #[http_body(description: "Optional expiration moment as unix microseconds. Omit for a rule that never expires.")]
    pub expiration: Option<i64>,
}

#[derive(Debug, MyHttpInput)]
pub struct DeleteIgnoreMaskHttpInput {
    #[http_query(description: "Log Level")]
    pub level: LogLevelHttpModel,

    #[http_query(description: "Application name")]
    pub application: String,

    #[http_query(description: "Filter marker")]
    pub marker: String,
}

#[derive(Debug, MyHttpObjectStructure, Serialize)]
pub struct IgnoreEventHttpModel {
    pub level: String,
    pub application: String,
    pub marker: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<i64>,
}
