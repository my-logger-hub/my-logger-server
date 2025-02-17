use my_http_server::macros::{MyHttpInput, MyHttpObjectStructure};
use serde::Serialize;

use crate::{
    http::controllers::shared_contract::LogLevelHttpModel, repo::ignore_events::IgnoreEventModel,
};

#[derive(Debug, MyHttpInput)]
pub struct PostIgnoreMaskHttpInput {
    #[http_body(description: "Log Level")]
    pub level: LogLevelHttpModel,

    #[http_body(description: "Application name")]
    pub application: String,

    #[http_body(description: "Filter marker")]
    pub marker: String,
}

impl Into<IgnoreEventModel> for PostIgnoreMaskHttpInput {
    fn into(self) -> IgnoreEventModel {
        IgnoreEventModel {
            level: self.level.into(),
            application: self.application,
            marker: self.marker,
        }
    }
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

impl Into<IgnoreEventModel> for DeleteIgnoreMaskHttpInput {
    fn into(self) -> IgnoreEventModel {
        IgnoreEventModel {
            level: self.level.into(),
            application: self.application,
            marker: self.marker,
        }
    }
}

#[derive(Debug, MyHttpObjectStructure, Serialize)]
pub struct IgnoreEventHttpModel {
    pub level: String,
    pub application: String,
    pub marker: String,
}

impl Into<IgnoreEventHttpModel> for IgnoreEventModel {
    fn into(self) -> IgnoreEventHttpModel {
        IgnoreEventHttpModel {
            level: self.level.as_str().to_string(),
            application: self.application,
            marker: self.marker,
        }
    }
}
