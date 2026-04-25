use std::sync::Arc;

use rust_extensions::MyTimerTick;

use crate::app::AppContext;

pub struct PersistStatisticsTimer {
    pub app: Arc<AppContext>,
}

impl PersistStatisticsTimer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for PersistStatisticsTimer {
    async fn tick(&self) {
        let snapshot = {
            let read_access = self.app.hourly_statistics.lock().await;
            read_access.snapshot()
        };

        let bytes = match serde_json::to_vec_pretty(&snapshot) {
            Ok(b) => b,
            Err(e) => {
                println!("Failed to serialize statistics: {}", e);
                return;
            }
        };

        if let Err(e) = tokio::fs::write(&self.app.statistics_path, bytes).await {
            println!(
                "Failed to write statistics to {}: {}",
                self.app.statistics_path, e
            );
        }
    }
}
