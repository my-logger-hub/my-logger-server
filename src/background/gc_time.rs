use std::sync::Arc;

use rust_extensions::{date_time::DateTimeAsMicroseconds, MyTimerTick};

use crate::app::AppContext;

pub struct GcTimer {
    pub app: Arc<AppContext>,
    pub tenant: String,
}

impl GcTimer {
    pub async fn new(app: Arc<AppContext>) -> Self {
        let tenant = app.settings_reader.get_default_tenant().await;
        Self { app, tenant }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for GcTimer {
    async fn tick(&self) {
        let mut to_date = DateTimeAsMicroseconds::now();
        to_date.add_days(-1);

        self.app.logs_repo.gc(&self.tenant, to_date).await;
    }
}
