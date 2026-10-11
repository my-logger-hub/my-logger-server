use std::sync::Arc;

use rust_extensions::{date_time::DateTimeAsMicroseconds, MyTimerTick, RepeatTimerIteration};

use crate::app::AppContext;

pub struct GcTimer {
    pub app: Arc<AppContext>,
}

impl GcTimer {
    pub async fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for GcTimer {
    async fn tick(&self) -> RepeatTimerIteration {
        if let Some(stats) = crate::app::read_jemalloc_stats() {
            self.app.debug_console.push(stats.to_debug_string()).await;
        }

        {
            let mut hourly_statistics = self.app.hourly_statistics.lock().await;
            hourly_statistics.gc();
        }

        let hours_to_gc = self.app.settings_repo.get_hours_to_gc().await;
        let storage = self.app.logs_storage.clone();

        let messages = tokio::task::spawn_blocking(move || {
            storage.gc(DateTimeAsMicroseconds::now(), hours_to_gc)
        })
        .await
        .unwrap();

        for message in messages {
            self.app.debug_console.push(message).await;
        }

        self.app.ignore_single_event_cache.lock().await.gc();

        if self.app.settings_repo.gc_expired().await {
            self.app.filter_events_cache.reset().await;
        }

        RepeatTimerIteration::WithInterval
    }
}
