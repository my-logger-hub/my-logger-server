use std::{collections::BTreeMap, sync::Arc};

use my_logger::LogLevel;
use rust_extensions::file_utils::FilePath;

use super::IgnoreSingleEventModel;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct IgnoreSingleEventFileContract {
    pub id: String,
    pub levels: Vec<String>,
    pub message_match: String,
    pub ctx_match: BTreeMap<String, String>,
    pub skip_amount: usize,
    pub minutes_to_wait: i64,
}

pub async fn save(file_path: &FilePath, items: &[Arc<IgnoreSingleEventModel>]) {
    let contracts: Vec<_> = items
        .iter()
        .map(|itm| IgnoreSingleEventFileContract {
            id: itm.id.to_string(),
            levels: itm
                .levels
                .iter()
                .map(|itm| itm.as_str().to_string())
                .collect(),
            message_match: itm.message_match.to_string(),
            skip_amount: itm.skip_amount,
            minutes_to_wait: itm.minutes_to_wait,
            ctx_match: itm.ctx_match.clone(),
        })
        .collect();

    let content = serde_json::to_vec(&contracts).unwrap();

    tokio::fs::write(file_path.as_str(), content.as_slice())
        .await
        .unwrap();
}

pub async fn load(file_path: &FilePath) -> Vec<Arc<IgnoreSingleEventModel>> {
    let items = match tokio::fs::read(file_path.as_str()).await {
        Ok(content) => match serde_json::from_slice::<Vec<IgnoreSingleEventFileContract>>(&content)
        {
            Ok(content) => content,
            Err(_) => vec![],
        },
        Err(_) => vec![],
    };

    items
        .into_iter()
        .map(|itm| {
            IgnoreSingleEventModel {
                levels: itm
                    .levels
                    .iter()
                    .map(|itm| LogLevel::from_str(itm))
                    .collect(),
                message_match: itm.message_match.to_string(),
                skip_amount: itm.skip_amount,
                minutes_to_wait: itm.minutes_to_wait,
                ctx_match: itm.ctx_match.clone(),
                id: itm.id,
            }
            .into()
        })
        .collect()
}
