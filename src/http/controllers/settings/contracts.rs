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
}

#[derive(Debug, MyHttpObjectStructure, Serialize)]
pub struct IgnoreEventHttpModel {
    pub level: String,
    pub application: String,
    pub marker: String,
}
