use std::sync::Arc;

use rust_extensions::file_utils::FilePath;
use tokio::sync::RwLock;

use crate::log_item::LogEvent;

use super::IgnoreSingleEventModel;

pub struct IgnoreSingleEventsRepo {
    file_path: FilePath,
    items: RwLock<Vec<Arc<IgnoreSingleEventModel>>>,
}

impl IgnoreSingleEventsRepo {
    pub async fn new(mut file_path: FilePath) -> Self {
        file_path.append_segment("ignore_single_events");

        let items = super::persistence::load(&file_path).await;

        Self {
            file_path,
            items: RwLock::new(items),
        }
    }

    pub async fn add(&self, item: IgnoreSingleEventModel) {
        let mut access = self.items.write().await;
        access.push(item.into());
        super::persistence::save(&self.file_path, &access).await;
    }

    pub async fn get_all(&self) -> Vec<Arc<IgnoreSingleEventModel>> {
        let read_access = self.items.read().await;
        read_access.clone()
    }

    pub async fn delete(&self, id: &str) {
        let mut access = self.items.write().await;

        let len_before = access.len();
        access.retain(|itm| itm.id.as_str() != id);

        if len_before != access.len() {
            super::persistence::save(&self.file_path, &access).await;
        }
    }

    pub async fn find_matching_events<'s>(
        &self,
        events: &'s [Arc<LogEvent>],
    ) -> Vec<(Arc<LogEvent>, Arc<IgnoreSingleEventModel>)> {
        let access = self.items.read().await;
        let mut result = Vec::new();

        for log_event in events {
            for ignore_event in access.iter() {
                if ignore_event.matches_to(log_event.as_ref()) {
                    result.push((log_event.clone(), ignore_event.clone()));
                    break;
                }
            }
        }

        result
    }
}
