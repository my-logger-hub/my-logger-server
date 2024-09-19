use std::collections::HashMap;

use serde::*;

use crate::{app::AppContext, my_logger_grpc::*};

const FILE_NAME: &'static str = "one-time-skip.yaml";

pub async fn save(app: &AppContext) {
    let items = app.ignore_single_event_cache.lock().await.get_all();

    let items_to_save: Vec<IgnoreSingleEventFileModel> =
        items.into_iter().map(|itm| itm.into()).collect();

    let as_yaml = serde_yaml::to_string(&items_to_save).unwrap();

    let file_name = app.settings_reader.get_logs_db_path(FILE_NAME.into()).await;

    println!("Saving ignore_single_event to {:?}", file_name);

    tokio::fs::write(file_name, as_yaml).await.unwrap();
}

pub async fn get_all(app: &AppContext) -> Vec<IgnoreSingleEventGrpcModel> {
    let file_name = app.settings_reader.get_logs_db_path(FILE_NAME.into()).await;
    let as_yaml = tokio::fs::read_to_string(file_name).await;

    if as_yaml.is_err() {
        return vec![];
    }

    let as_yaml = as_yaml.unwrap();

    let items: Vec<IgnoreSingleEventFileModel> = serde_yaml::from_str(&as_yaml).unwrap();

    items.into_iter().map(|itm| itm.into()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgnoreSingleEventFileModel {
    pub id: String,
    pub levels: Vec<i32>,
    pub message_match: String,
    pub context_match: HashMap<String, String>,
    pub skip_amount: u64,
    pub minutes_to_wait: u64,
}

impl Into<IgnoreSingleEventFileModel> for IgnoreSingleEventGrpcModel {
    fn into(self) -> IgnoreSingleEventFileModel {
        IgnoreSingleEventFileModel {
            id: self.id,
            levels: self.levels,
            message_match: self.message_match,
            context_match: self
                .context_match
                .into_iter()
                .map(|itm| (itm.key, itm.value))
                .collect(),
            skip_amount: self.skip_amount,
            minutes_to_wait: self.minutes_to_wait,
        }
    }
}

impl Into<IgnoreSingleEventGrpcModel> for IgnoreSingleEventFileModel {
    fn into(self) -> IgnoreSingleEventGrpcModel {
        IgnoreSingleEventGrpcModel {
            id: self.id,
            levels: self.levels,
            message_match: self.message_match,
            context_match: self
                .context_match
                .into_iter()
                .map(|itm| LogEventContext {
                    key: itm.0,
                    value: itm.1,
                })
                .collect(),
            skip_amount: self.skip_amount,
            minutes_to_wait: self.minutes_to_wait,
        }
    }
}
