use my_logger::LogLevel;
use rust_extensions::file_utils::FilePath;

use super::IgnoreEventModel;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IgnoreEventFileContract {
    pub level: String,
    pub application: String,
    pub marker: String,
}

pub async fn save(file_path: &FilePath, items: &[IgnoreEventModel]) {
    let contracts: Vec<_> = items
        .iter()
        .map(|itm| IgnoreEventFileContract {
            level: itm.level.as_str().to_string(),
            application: itm.application.to_string(),
            marker: itm.marker.to_string(),
        })
        .collect();

    let content = serde_json::to_vec(&contracts).unwrap();

    tokio::fs::write(file_path.as_str(), content.as_slice())
        .await
        .unwrap();
}

pub async fn load(file_path: &FilePath) -> Vec<IgnoreEventModel> {
    let items = match tokio::fs::read(file_path.as_str()).await {
        Ok(content) => match serde_json::from_slice::<Vec<IgnoreEventFileContract>>(&content) {
            Ok(content) => content,
            Err(_) => vec![],
        },
        Err(_) => vec![],
    };

    items
        .into_iter()
        .map(|itm| IgnoreEventModel {
            level: LogLevel::from_str(&itm.level),
            application: itm.application,
            marker: itm.marker,
        })
        .collect()
}
