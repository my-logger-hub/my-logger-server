use std::collections::BTreeMap;

use my_http_server::{macros::*, types::RawDataTyped, HttpFailResult};
use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::Deserialize;

use crate::app::LogItem;

#[derive(MyHttpInput)]
pub struct PostJsonLogsV2InputData {
    #[http_body_raw(description = "The Seq of the request")]
    pub body: RawDataTyped<Vec<JsonHttpLogItem>>,
}

impl PostJsonLogsV2InputData {
    pub fn parse_log_events(&self, tenant: &str) -> Result<Vec<LogItem>, HttpFailResult> {
        let items = self.body.deserialize_json()?;

        let mut result = Vec::with_capacity(items.len());

        for itm in items {
            let timestamp = DateTimeAsMicroseconds::from_str(&itm.time_stamp);

            if timestamp.is_none() {
                return Err(HttpFailResult::as_validation_error(format!(
                    "Invalid time_stamp for entity: {:#?}",
                    itm
                )));
            }

            let timestamp = timestamp.unwrap();

            result.push(LogItem {
                id: crate::utils::generate_log_id(),
                tenant: tenant.to_string(),
                level: parse_log_level(&itm.level)?,
                process: itm.process,
                message: itm.message,
                timestamp,
                ctx: if let Some(ctx) = itm.context {
                    ctx
                } else {
                    BTreeMap::new()
                },
            });
        }

        Ok(result)
    }
}

#[derive(MyHttpInputObjectStructure, Deserialize, Debug)]
pub struct JsonHttpLogItem {
    pub time_stamp: String,
    pub level: String,
    pub process: Option<String>,
    pub message: String,
    pub context: Option<BTreeMap<String, String>>,
}

fn parse_log_level(src: &str) -> Result<LogLevel, HttpFailResult> {
    match src {
        "Info" => Ok(LogLevel::Info),
        "Warning" => Ok(LogLevel::Warning),
        "Error" => Ok(LogLevel::Error),
        "FatalError" => Ok(LogLevel::FatalError),
        "Debug" => Ok(LogLevel::Debug),
        _ => Err(HttpFailResult::as_validation_error(format!(
            "Invalid log level {}",
            src
        ))),
    }
}
