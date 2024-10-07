use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

use rust_extensions::MyTimerTick;

use crate::{
    app::{AppContext, LogItem, PROCESS_CONTEXT_KEY},
    repo::{dto::LogItemDto, DateHourKey},
};

pub struct FlushToDbTimer {
    pub app: Arc<AppContext>,
}

impl FlushToDbTimer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }

    async fn send_to_telegram_if_needed(&self, items: &VecDeque<Arc<LogItem>>) {
        let mut to_send = Vec::new();

        for itm in items {
            match &itm.level {
                my_logger::LogLevel::Info => {}
                my_logger::LogLevel::Warning => {}
                my_logger::LogLevel::Error => {
                    to_send.push(itm);
                }
                my_logger::LogLevel::FatalError => {
                    to_send.push(itm);
                }
                my_logger::LogLevel::Debug => {}
            }
        }

        if to_send.len() == 0 {
            return;
        }

        let telegram_settings = self.app.settings_reader.get_telegram_settings().await;

        if telegram_settings.is_none() {
            return;
        }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for FlushToDbTimer {
    async fn tick(&self) {
        crate::flows::write_hour_statistics(&self.app).await;

        while let Some(items) = self.app.logs_queue.get(1000).await {
            self.send_to_telegram_if_needed(&items).await;

            let mut to_upload: BTreeMap<DateHourKey, Vec<LogItemDto>> = BTreeMap::new();

            for item in items {
                let date_key = DateHourKey::new(item.timestamp);

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
                self.app.logs_repo.upload(date_key, items.as_slice()).await;
            }
        }
    }
}

impl<'s> Into<LogItemDto> for &'s LogItem {
    fn into(self) -> LogItemDto {
        let mut context = self.ctx.clone();
        if let Some(process) = self.process.as_ref() {
            context.insert(PROCESS_CONTEXT_KEY.to_string(), process.to_string());
        }

        LogItemDto {
            id: self.id.to_string(),
            level: (&self.level).into(),
            message: self.message.to_string(),
            moment: self.timestamp,
            context,
        }
    }
}
