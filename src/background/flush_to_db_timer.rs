use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
};

use rust_extensions::MyTimerTick;

use crate::{
    app::{AppContext, LogItem, PROCESS_CONTEXT_KEY},
    repo::{dto::LogItemDto, DateKey},
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
            crate::telegram_api::send_log_item(&telegram_settings, itm).await;
        }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for FlushToDbTimer {
    async fn tick(&self) {
        while let Some(items) = self.app.logs_queue.get(1000).await {
            self.send_to_telegram_if_needed(&items).await;

            let mut to_upload: HashMap<String, BTreeMap<DateKey, Vec<LogItemDto>>> = HashMap::new();

            for item in items {
                let date_key = DateKey::new(item.timestamp);

                if !to_upload.contains_key(&item.tenant) {
                    to_upload.insert(item.tenant.to_string(), BTreeMap::new());
                }

                let by_tenant = to_upload.get_mut(&item.tenant).unwrap();

                if by_tenant.contains_key(&date_key) {
                    by_tenant.get_mut(&date_key).unwrap().push(item.into());
                } else {
                    by_tenant.insert(date_key, vec![item.into()]);
                }
            }

            for (tenant, items) in to_upload {
                for (date_key, items) in items {
                    self.app
                        .logs_repo
                        .upload(tenant.as_str(), date_key, items.as_slice())
                        .await;
                }
            }
        }
    }
}

impl Into<LogItemDto> for LogItem {
    fn into(self) -> LogItemDto {
        let mut context = self.ctx;
        if let Some(process) = self.process {
            context.insert(PROCESS_CONTEXT_KEY.to_string(), process);
        }

        LogItemDto {
            id: self.id,
            level: self.level.into(),
            message: self.message,
            moment: self.timestamp,
            context,
        }
    }
}
