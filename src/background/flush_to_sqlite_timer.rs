use std::{collections::BTreeMap, sync::Arc};

use rust_extensions::MyTimerTick;

use crate::{
    app::{AppContext, PROCESS_CONTEXT_KEY},
    repo::{dto::{LogItemDto, LogLevelDto}, DateHourKey},
};

pub struct FlushToSqliteTimer {
    pub app: Arc<AppContext>,
}

impl FlushToSqliteTimer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for FlushToSqliteTimer {
    async fn tick(&self) {
        while let Some(items) = self.app.sqlite_logs_queue.get(1000).await {
            let mut to_upload: BTreeMap<(LogLevelDto, DateHourKey), Vec<LogItemDto>> =
                BTreeMap::new();

            for item in items {
                let date_key = DateHourKey::new(item.timestamp);
                let level: LogLevelDto = (&item.level).into();

                let mut context = item.ctx.clone();
                if let Some(process) = item.process.as_ref() {
                    context.insert(PROCESS_CONTEXT_KEY.to_string(), process.to_string());
                }
                let dto = LogItemDto {
                    id: item.id.to_string(),
                    level: level.clone(),
                    message: item.message.to_string(),
                    moment: item.timestamp,
                    context,
                };

                to_upload
                    .entry((level, date_key))
                    .or_insert_with(Vec::new)
                    .push(dto);
            }

            for ((level, date_key), items) in to_upload {
                self.app
                    .sqlite_logs_repo
                    .upload(level, date_key, items.as_slice())
                    .await;
            }
        }
    }
}
