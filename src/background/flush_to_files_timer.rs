use std::sync::Arc;

use rust_extensions::{MyTimerTick, RepeatTimerIteration};

use crate::app::AppContext;

pub struct FlushToFilesTimer {
    app: Arc<AppContext>,
}

impl FlushToFilesTimer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for FlushToFilesTimer {
    async fn tick(&self) -> RepeatTimerIteration {
        crate::flows::flush_logs_to_files(&self.app, false).await;
        RepeatTimerIteration::WithInterval
    }
}
