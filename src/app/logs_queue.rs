use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::repo::dto::LogLevelDto;

#[derive(Debug)]
pub struct LogItem {
    pub id: String,
    pub level: LogLevel,
    pub process: Option<String>,
    pub message: String,
    pub timestamp: DateTimeAsMicroseconds,
    pub ctx: BTreeMap<String, String>,
}

impl LogItem {
    pub fn is_application(&self, application: &str) -> bool {
        if let Some(app_name) = self.ctx.get("Application") {
            return app_name == application;
        }

        false
    }

    pub fn has_entry(&self, entry: &str) -> bool {
        if let Some(process) = &self.process {
            return process.contains(entry) || self.message.contains(entry);
        }

        self.message.contains(entry)
    }

    pub fn is_level(&self, level: &LogLevelDto) -> bool {
        match level {
            LogLevelDto::Info => level.is_info(),
            LogLevelDto::Warning => level.is_warning(),
            LogLevelDto::Error => level.is_error(),
            LogLevelDto::FatalError => level.is_fatal_error(),
            LogLevelDto::Debug => level.is_debug(),
        }
    }
}

pub struct LogsQueue {
    pub queue: tokio::sync::Mutex<Option<VecDeque<Arc<LogItem>>>>,
}

impl LogsQueue {
    pub fn new() -> Self {
        Self {
            queue: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn add(&self, items: Vec<Arc<LogItem>>) {
        //println!("Added events: {}", items.len());

        let mut write_access = self.queue.lock().await;
        if write_access.is_none() {
            *write_access = Some(VecDeque::new());
        }

        write_access.as_mut().unwrap().extend(items);
    }

    pub async fn get(&self, max_items_to_dequeue: usize) -> Option<VecDeque<Arc<LogItem>>> {
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

/*

    pub async fn get_elastic(&self, max_items_to_dequeue: usize) -> Option<VecDeque<serde_json::Value>> {
        let mut write_access = self.elastic_queue.lock().await;

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
*/
