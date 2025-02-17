use std::sync::Arc;

use super::*;

pub struct LogsQueue {
    pub queue: tokio::sync::Mutex<Option<Vec<Arc<LogEvent>>>>,
}

impl LogsQueue {
    pub fn new() -> Self {
        Self {
            queue: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn add(&self, items: Vec<Arc<LogEvent>>) {
        //println!("Added events: {}", items.len());

        let mut write_access = self.queue.lock().await;
        if write_access.is_none() {
            *write_access = Some(Vec::new());
        }

        write_access.as_mut().unwrap().extend(items);
    }

    pub async fn get(&self, max_items_to_dequeue: usize) -> Option<Vec<Arc<LogEvent>>> {
        let mut write_access = self.queue.lock().await;

        if write_access.is_none() {
            return None;
        }

        let items_in_queue = write_access.as_ref().unwrap().len();

        if max_items_to_dequeue <= items_in_queue {
            return write_access.take();
        }

        let result = write_access
            .as_mut()
            .unwrap()
            .drain(..max_items_to_dequeue)
            .collect();

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
