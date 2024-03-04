use tokio::sync::RwLock;

use crate::repo::dto::IgnoreItemDto;

pub enum FilterEventResult<T> {
    Ok(T),
    NotInitialized(T),
}

pub struct FilterEventsCache {
    items: RwLock<Option<Vec<IgnoreItemDto>>>,
}

impl FilterEventsCache {
    pub fn new() -> Self {
        Self {
            items: RwLock::new(None),
        }
    }

    pub async fn apply(&self, items: Vec<IgnoreItemDto>) {
        let mut write_access = self.items.write().await;
        *write_access = Some(items);
    }

    pub async fn reset(&self) {
        let mut write_access = self.items.write().await;
        *write_access = None;
    }

    pub async fn filter_events<T>(
        &self,
        events: Vec<T>,
        filter: impl Fn(&T, &[IgnoreItemDto]) -> bool,
    ) -> FilterEventResult<Vec<T>> {
        let read_access = self.items.read().await;

        let items = read_access.as_ref();

        if items.is_none() {
            return FilterEventResult::NotInitialized(events);
        }

        let items = items.unwrap();

        let mut result = Vec::with_capacity(events.len());

        for itm in events {
            if filter(&itm, items.as_ref()) {
                result.push(itm);
            }
        }

        FilterEventResult::Ok(result)
    }
}
