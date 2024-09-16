use std::collections::BTreeMap;

use my_http_server::{macros::*, types::RawDataTyped, HttpFailResult};
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::Deserialize;

use crate::{app::LogItem, http::controllers::shared_contract::LogLevelHttpModel};

#[derive(MyHttpInput)]
pub struct PostJsonLogsV2InputData {
    #[http_body_raw(description = "The Seq of the request")]
    pub body: RawDataTyped<Vec<JsonHttpLogItem>>,
}

impl PostJsonLogsV2InputData {
    pub fn parse_log_events(&self) -> Result<Vec<LogItem>, HttpFailResult> {
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
                level: itm.level.into(),
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
    pub level: LogLevelHttpModel,
    pub process: Option<String>,
    pub message: String,
    pub context: Option<BTreeMap<String, String>>,
}
