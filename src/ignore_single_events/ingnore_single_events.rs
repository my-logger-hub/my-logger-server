use std::time::Duration;

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::LogItem, my_logger_grpc::*};

pub struct IgnoreSingleEventItem {
    pub item: IgnoreSingleEventGrpcModel,
    pub prev_events: Vec<DateTimeAsMicroseconds>,
}

pub struct IgnoreSingleEventCache {
    data: Vec<IgnoreSingleEventItem>,
}

impl IgnoreSingleEventCache {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn add(&mut self, item: IgnoreSingleEventGrpcModel) {
        self.data.retain(|data_itm| data_itm.item.id != item.id);
        self.data.push(IgnoreSingleEventItem {
            item,
            prev_events: vec![],
        });
    }

    pub fn get_all(&self) -> Vec<IgnoreSingleEventGrpcModel> {
        self.data.iter().map(|itm| itm.item.clone()).collect()
    }

    pub fn delete(&mut self, id: &str) {
        self.data.retain(|itm| itm.item.id != id);
    }

    pub fn skip_by_filtering(&mut self, itm: &LogItem) -> bool {
        for delay_itm in self.data.iter_mut() {
            if !super::event_matching::match_event(itm, &delay_itm.item) {
                continue;
            }

            delay_itm.prev_events.push(itm.timestamp.clone());

            let items_before_alerting = delay_itm.item.skip_amount as usize;
            let skip = delay_itm.prev_events.len() <= items_before_alerting;

            while delay_itm.prev_events.len() > items_before_alerting + 1 {
                delay_itm.prev_events.remove(0);
            }

            return skip;
        }

        false
    }

    pub fn gc(&mut self) {
        for itm in &mut self.data {
            let mut to_deleted: Vec<DateTimeAsMicroseconds> = Vec::new();

            let gc_moment = DateTimeAsMicroseconds::now()
                .sub(Duration::from_secs(itm.item.minutes_to_wait * 60));

            for itm in itm.prev_events.iter() {
                if itm < &gc_moment {
                    to_deleted.push(itm.clone());
                }
            }

            for to_delete in to_deleted {
                itm.prev_events.retain(|x| x != &to_delete);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::collections::BTreeMap;

    use rust_extensions::date_time::DateTimeAsMicroseconds;

    use crate::app::LogItem;

    use super::{IgnoreSingleEventCache, LogLevelGrpcModel};

    #[test]
    fn test_simple_case() {
        let mut cache = IgnoreSingleEventCache::new();

        cache.add(super::IgnoreSingleEventGrpcModel {
            id: "1".to_string(),
            levels: vec![LogLevelGrpcModel::Error as i32],
            message_match: "Test".to_string(),
            context_match: vec![],
            skip_amount: 2,
            minutes_to_wait: 1,
        });

        let mut now = DateTimeAsMicroseconds::now();

        let log_item = LogItem {
            id: "Test".to_string(),
            tenant: "Test".to_string(),
            level: my_logger::LogLevel::Error,
            process: Some("Test".to_string()),
            message: "Test".to_string(),
            timestamp: now,
            ctx: BTreeMap::new(),
        };
        let skip = cache.skip_by_filtering(&log_item);
        assert_eq!(skip, true);

        now.add_seconds(1);
        let log_item = LogItem {
            id: "Test".to_string(),
            tenant: "Test".to_string(),
            level: my_logger::LogLevel::Error,
            process: Some("Test".to_string()),
            message: "Test".to_string(),
            timestamp: now,
            ctx: BTreeMap::new(),
        };
        let skip = cache.skip_by_filtering(&log_item);
        assert_eq!(skip, true);

        now.add_seconds(1);
        let log_item = LogItem {
            id: "Test".to_string(),
            tenant: "Test".to_string(),
            level: my_logger::LogLevel::Error,
            process: Some("Test".to_string()),
            message: "Test".to_string(),
            timestamp: now,
            ctx: BTreeMap::new(),
        };
        let skip = cache.skip_by_filtering(&log_item);
        assert_eq!(skip, false);
    }
}
