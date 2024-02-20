use std::collections::{BTreeMap, VecDeque};

use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;

#[derive(Debug)]
pub struct LogItem {
    pub id: String,
    pub tenant: String,
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

    pub fn is_level(&self, level: &str) -> bool {
        match &self.level {
            my_logger::LogLevel::Info => {
                rust_extensions::str_utils::compare_strings_case_insensitive(level, "info")
            }
            my_logger::LogLevel::Warning => {
                rust_extensions::str_utils::compare_strings_case_insensitive(level, "warning")
            }
            my_logger::LogLevel::Error => {
                rust_extensions::str_utils::compare_strings_case_insensitive(level, "error")
            }
            my_logger::LogLevel::FatalError => {
                rust_extensions::str_utils::compare_strings_case_insensitive(level, "fatalerror")
            }
            my_logger::LogLevel::Debug => {
                rust_extensions::str_utils::compare_strings_case_insensitive(level, "debug")
            }
        }
    }
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
