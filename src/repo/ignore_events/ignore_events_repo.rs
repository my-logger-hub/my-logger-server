use rust_extensions::file_utils::FilePath;
use tokio::sync::RwLock;

use crate::log_item::LogEvent;

use super::*;

pub struct IgnoreEventsRepo {
    file_path: FilePath,
    items: RwLock<Vec<IgnoreEventModel>>,
}

impl IgnoreEventsRepo {
    pub async fn new(mut file_path: FilePath) -> Self {
        file_path.append_segment("ignore_events");

        let items = super::persistence::load(&file_path).await;

        Self {
            file_path,
            items: RwLock::new(items),
        }
    }
    pub async fn add(&self, item: IgnoreEventModel) {
        let mut write_access = self.items.write().await;
        write_access.push(item);

        super::persistence::save(&self.file_path, &write_access).await;
    }

    pub async fn get_all(&self) -> Vec<IgnoreEventModel> {
        let read_access = self.items.read().await;
        read_access.clone()
    }

    pub async fn remove(&self, item: &IgnoreEventModel) {
        let mut write_access = self.items.write().await;
        let before = write_access.len();
        write_access.retain(|itm| !itm.is_same(&item));

        if before != write_access.len() {
            super::persistence::save(&self.file_path, &write_access).await;
        }
    }

    pub async fn filter(&self, logs: &mut Vec<LogEvent>) {
        let read_access = self.items.read().await;

        logs.retain(|log_event| {
            for ignore in read_access.iter() {
                if ignore.ignore_me(log_event) {
                    return false;
                }
            }

            true
        });
    }
}
