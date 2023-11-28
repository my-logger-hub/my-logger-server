use std::collections::VecDeque;

use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct LogCtxItem {
    pub key: String,
    pub value: String,
}

#[derive(Debug)]
pub struct LogItem {
    pub id: String,
    pub tenant: String,
    pub level: LogLevel,
    pub process: Option<String>,
    pub message: String,
    pub timestamp: DateTimeAsMicroseconds,
    pub ctx: Vec<LogCtxItem>,
}

pub struct LogsQueue {
    pub queue: tokio::sync::Mutex<Option<VecDeque<LogItem>>>,
}

impl LogsQueue {
    pub fn new() -> Self {
        Self {
            queue: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn add(&self, items: Vec<LogItem>) {
        println!("Added events: {}", items.len());
        let mut write_access = self.queue.lock().await;
        if write_access.is_none() {
            *write_access = Some(VecDeque::new());
        }

        write_access.as_mut().unwrap().extend(items);
    }

    pub async fn get(&self, max_items_to_dequeue: usize) -> Option<VecDeque<LogItem>> {
        let mut write_access = self.queue.lock().await;

        if write_access.is_none() {
            return None;
        }

        if write_access.as_ref().unwrap().len() <= max_items_to_dequeue {
            return write_access.take();
        }

        let mut result = VecDeque::with_capacity(max_items_to_dequeue);

        while result.len() < max_items_to_dequeue {
            let item = write_access.as_mut().unwrap().pop_front().unwrap();
            result.push_back(item);
        }
        Some(result)
    }
}
