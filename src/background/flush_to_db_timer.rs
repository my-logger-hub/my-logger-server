use std::{collections::BTreeMap, sync::Arc};

use rust_extensions::MyTimerTick;

use crate::{
    app::AppContext,
    repo::logs::{LogEventFileGrpcModel, TenMinKey},
};

pub struct FlushToDbTimer {
    pub app: Arc<AppContext>,
}

impl FlushToDbTimer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for FlushToDbTimer {
    async fn tick(&self) {
        while let Some(items) = self.app.logs_queue.get(1000).await {
            self.app
                .telegram_notification_data
                .update(items.as_slice())
                .await;

            let mut to_upload: BTreeMap<TenMinKey, Vec<LogEventFileGrpcModel>> = BTreeMap::new();

            for item in items {
                let date_key = TenMinKey::new(item.timestamp);

                if to_upload.contains_key(&date_key) {
                    to_upload
                        .get_mut(&date_key)
                        .unwrap()
                        .push(item.as_ref().into());
                } else {
                    to_upload.insert(date_key, vec![item.as_ref().into()]);
                }
            }

            for (date_key, items) in to_upload {
                self.app
                    .logs_repo
                    .upload_logs(date_key, items.as_slice())
                    .await;
            }
        }
    }
}
