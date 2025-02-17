use std::{collections::BTreeMap, sync::Arc};

use my_logger::LogLevel;
use rust_extensions::{date_time::*, sorted_vec::EntityWithKey};
use tokio::sync::Mutex;

use crate::log_item::*;

#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub key: IntervalKey<MinuteKey>,
    pub fatal_errors: usize,
    pub errors: usize,
    pub warnings: usize,
}

impl EntityWithKey<i64> for NotificationItem {
    fn get_key(&self) -> &i64 {
        self.key.as_i64_ref()
    }
}

pub struct TelegramNotificationData {
    items: Mutex<BTreeMap<i64, NotificationItem>>,
}

impl TelegramNotificationData {
    pub fn new() -> Self {
        Self {
            items: Mutex::default(),
        }
    }

    pub async fn update(&self, items: &[Arc<LogEvent>]) {
        let mut write_access = self.items.lock().await;
        for itm in items {
            match itm.level {
                LogLevel::FatalError => inc_fatal_error(&mut write_access, itm.timestamp),
                LogLevel::Error => inc_error(&mut write_access, itm.timestamp),
                LogLevel::Warning => inc_warnings(&mut write_access, itm.timestamp),
                _ => {}
            }
        }
    }

    pub async fn get_something_to_notify(
        &self,
        mut now: DateTimeAsMicroseconds,
    ) -> Option<NotificationItem> {
        let mut write_access = self.items.lock().await;
        now.add_minutes(-1);

        let key_to_send: IntervalKey<MinuteKey> = now.into();

        let mut result_key = None;

        for item_key in write_access.keys() {
            if *item_key < key_to_send.to_i64() {
                result_key = Some(*item_key);
            }

            break;
        }

        let result_key = result_key?;
        write_access.remove(&result_key)
    }
}

pub fn inc_fatal_error(data: &mut BTreeMap<i64, NotificationItem>, dt: DateTimeAsMicroseconds) {
    let key: IntervalKey<MinuteKey> = dt.into();

    let key_i64 = key.to_i64();
    match data.get_mut(&key_i64) {
        Some(item) => {
            item.fatal_errors += 1;
        }
        None => {
            data.insert(
                key_i64,
                NotificationItem {
                    key,
                    fatal_errors: 1,
                    errors: 0,
                    warnings: 0,
                },
            );
        }
    }
}

pub fn inc_error(data: &mut BTreeMap<i64, NotificationItem>, dt: DateTimeAsMicroseconds) {
    let key: IntervalKey<MinuteKey> = dt.into();

    let key_i64 = key.to_i64();
    match data.get_mut(&key_i64) {
        Some(item) => {
            item.errors += 1;
        }
        None => {
            data.insert(
                key_i64,
                NotificationItem {
                    key,
                    fatal_errors: 0,
                    errors: 1,
                    warnings: 0,
                },
            );
        }
    }
}

pub fn inc_warnings(data: &mut BTreeMap<i64, NotificationItem>, dt: DateTimeAsMicroseconds) {
    let key: IntervalKey<MinuteKey> = dt.into();

    let key_i64 = key.to_i64();
    match data.get_mut(&key_i64) {
        Some(item) => {
            item.warnings += 1;
        }
        None => {
            data.insert(
                key_i64,
                NotificationItem {
                    key: key,
                    fatal_errors: 0,
                    errors: 0,
                    warnings: 1,
                },
            );
        }
    }
}
