use std::{collections::VecDeque, sync::Arc};

use rust_extensions::MyTimerTick;

use crate::{
    app::{AppContext, LogItem},
    repo::dto::LogItemDto,
};

pub struct FlushToDbTimer {
    pub app: Arc<AppContext>,
}

impl FlushToDbTimer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }

    async fn send_to_telegram_if_needed(&self, items: &VecDeque<LogItem>) {
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

        let telegram_settings = telegram_settings.unwrap();

        for itm in to_send {
            crate::telegram_api::send_message(&telegram_settings, itm).await;
        }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for FlushToDbTimer {
    async fn tick(&self) {
        while let Some(items) = self.app.logs_queue.get(1000).await {
            self.send_to_telegram_if_needed(&items).await;

            let items = items
                .into_iter()
                .map(|item| item.into())
                .collect::<Vec<_>>();

            println!("Found items to upload: {}", items.len());

            self.app.logs_repo.upload(items.as_slice()).await;
        }
    }
}

impl Into<LogItemDto> for LogItem {
    fn into(self) -> LogItemDto {
        LogItemDto {
            id: self.id,
            tenant: self.tenant,
            level: self.level.into(),
            process: if let Some(process) = self.process {
                process
            } else {
                "".to_string()
            },
            message: self.message,
            moment: self.timestamp,
            context: self.ctx,
        }
    }
}
