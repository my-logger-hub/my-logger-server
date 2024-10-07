use std::collections::BTreeMap;

use my_logger::LogLevel;
use rust_extensions::{date_time::*, sorted_vec::EntityWithKey};

use crate::app::LogItem;

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
    items: BTreeMap<i64, NotificationItem>,
}

impl TelegramNotificationData {
    pub fn new() -> Self {
        Self {
            items: BTreeMap::new(),
        }
    }

    pub fn update(&mut self, itm: &LogItem) {
        match itm.level {
            LogLevel::FatalError => self.inc_fatal_error(itm.timestamp),
            LogLevel::Error => self.inc_error(itm.timestamp),
            LogLevel::Warning => self.inc_warnings(itm.timestamp),
            _ => {}
        }
    }

    pub fn inc_fatal_error(&mut self, dt: DateTimeAsMicroseconds) {
        let key: IntervalKey<MinuteKey> = dt.into();

        let key_i64 = key.to_i64();
        match self.items.get_mut(&key_i64) {
            Some(item) => {
                item.fatal_errors += 1;
            }
            None => {
                self.items.insert(
                    key_i64,
                    NotificationItem {
                        key: key,
                        fatal_errors: 1,
                        errors: 0,
                        warnings: 0,
                    },
                );
            }
        }
    }

    pub fn inc_error(&mut self, dt: DateTimeAsMicroseconds) {
        let key: IntervalKey<MinuteKey> = dt.into();

        let key_i64 = key.to_i64();
        match self.items.get_mut(&key_i64) {
            Some(item) => {
                item.errors += 1;
            }
            None => {
                self.items.insert(
                    key_i64,
                    NotificationItem {
                        key: key,
                        fatal_errors: 0,
                        errors: 1,
                        warnings: 0,
                    },
                );
            }
        }
    }

    pub fn inc_warnings(&mut self, dt: DateTimeAsMicroseconds) {
        let key: IntervalKey<MinuteKey> = dt.into();

        let key_i64 = key.to_i64();
        match self.items.get_mut(&key_i64) {
            Some(item) => {
                item.warnings += 1;
            }
            None => {
                self.items.insert(
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

    pub fn get_something_to_notify(
        &mut self,
        mut now: DateTimeAsMicroseconds,
    ) -> Option<NotificationItem> {
        now.add_minutes(-1);

        let key_to_send: IntervalKey<HourKey> = now.into();

        let mut result_key = None;

        for item_key in self.items.keys() {
            if *item_key <= key_to_send.to_i64() {
                result_key = Some(*item_key);
            }

            break;
        }

        let result_key = result_key?;
        self.items.remove(&result_key)
    }
}
