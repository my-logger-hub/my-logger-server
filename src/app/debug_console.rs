use std::collections::VecDeque;

use rust_extensions::date_time::DateTimeAsMicroseconds;
use tokio::sync::Mutex;

const MAX_MESSAGES: usize = 1000;

#[derive(Debug, Clone)]
pub struct DebugConsoleMessage {
    pub moment: DateTimeAsMicroseconds,
    pub message: String,
}

pub struct DebugConsole {
    messages: Mutex<VecDeque<DebugConsoleMessage>>,
}

impl DebugConsole {
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(VecDeque::new()),
        }
    }

    pub async fn push(&self, message: String) {
        let mut write_access = self.messages.lock().await;
        while write_access.len() >= MAX_MESSAGES {
            write_access.pop_front();
        }
        write_access.push_back(DebugConsoleMessage {
            moment: DateTimeAsMicroseconds::now(),
            message,
        });
    }

    pub async fn get_last(&self, limit: usize) -> Vec<DebugConsoleMessage> {
        let read_access = self.messages.lock().await;
        let skip = read_access.len().saturating_sub(limit);
        read_access.iter().skip(skip).cloned().collect()
    }
}
