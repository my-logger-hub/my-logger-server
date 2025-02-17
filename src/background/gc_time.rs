use std::sync::Arc;

use rust_extensions::{date_time::DateTimeAsMicroseconds, MyTimerTick};

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
    async fn tick(&self) {
        self.app.hour_statistics_repo.persist().await;
        self.app.hour_statistics_repo.gc().await;

        let gc_duration = self.app.settings_reader.get_duration_to_gc().await;

        let now = DateTimeAsMicroseconds::now();
        self.app.logs_repo.gc(now, gc_duration).await;
    }
}
