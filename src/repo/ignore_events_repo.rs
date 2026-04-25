use tokio::sync::Mutex;

use super::dto::*;

pub struct SettingsRepo {
    items: Mutex<Vec<IgnoreItemDto>>,
    path: String,
}

impl SettingsRepo {
    pub async fn new(path: String) -> Self {
        let items = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice::<Vec<IgnoreItemDto>>(&bytes).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Self {
            items: Mutex::new(items),
            path,
        }
    }

    pub async fn get_ignore_events(&self) -> Vec<IgnoreItemDto> {
        let read_access = self.items.lock().await;
        read_access.clone()
    }

    pub async fn add_ignore_event(&self, item: &IgnoreItemDto) {
        let mut write_access = self.items.lock().await;
        if write_access
            .iter()
            .any(|x| x.level == item.level && x.application == item.application && x.marker == item.marker)
        {
            return;
        }
        write_access.push(item.clone());
        persist(&self.path, &write_access).await;
    }

    pub async fn delete_ignore_event(&self, model: &IgnoreWhereModel) {
        let mut write_access = self.items.lock().await;
        let before = write_access.len();
        write_access.retain(|item| !model.matches(item));
        if write_access.len() != before {
            persist(&self.path, &write_access).await;
        }
    }
}

async fn persist(path: &str, items: &[IgnoreItemDto]) {
    let bytes = match serde_json::to_vec_pretty(items) {
        Ok(b) => b,
        Err(e) => {
            println!("Failed to serialize ignore events: {}", e);
            return;
        }
    };
    if let Err(e) = tokio::fs::write(path, bytes).await {
        println!("Failed to persist ignore events to {}: {}", path, e);
    }
}
