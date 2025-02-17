use std::{collections::HashMap, sync::Arc};

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::repo::ignore_single_events::IgnoreSingleEventModel;

pub struct IgnoreSingleEventCacheItem {
    pub started: DateTimeAsMicroseconds,
    pub amount: usize,
    pub item: Arc<IgnoreSingleEventModel>,
}

impl IgnoreSingleEventCacheItem {
    pub fn new(item: Arc<IgnoreSingleEventModel>) -> Self {
        Self {
            started: DateTimeAsMicroseconds::now(),
            amount: 1,
            item,
        }
    }

    pub fn expired(&self, now: DateTimeAsMicroseconds) -> bool {
        let duration = now - self.started;

        duration.get_full_minutes() > self.item.minutes_to_wait
    }
}

#[derive(Default)]
pub struct IgnoreSingleEventCache {
    data: HashMap<String, IgnoreSingleEventCacheItem>,
}

impl IgnoreSingleEventCache {
    pub fn skip_it(&mut self, item: &Arc<IgnoreSingleEventModel>) -> bool {
        let mut skip_it = false;
        let mut remove_item = false;
        match self.data.get_mut(&item.id) {
            Some(item) => {
                let now = DateTimeAsMicroseconds::now();
                if item.expired(now) {
                    remove_item = true
                } else {
                    item.amount += 1;
                    skip_it = item.amount >= item.item.skip_amount;
                    remove_item = !skip_it; //todo!("Unit test it")
                }
            }
            None => {
                self.data.insert(
                    item.id.clone(),
                    IgnoreSingleEventCacheItem::new(item.clone()),
                );
                skip_it = true
            }
        }

        if remove_item {
            self.data.remove(&item.id);
        }

        skip_it
    }
}
