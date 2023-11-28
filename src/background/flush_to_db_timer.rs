use std::{collections::BTreeMap, sync::Arc};

use rust_extensions::MyTimerTick;

use crate::{
    app::{AppContext, LogItem},
    postgres::dto::LogItemDto,
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
            let items = items
                .into_iter()
                .map(|item| item.into())
                .collect::<Vec<_>>();

            println!("Found items to upload: {}", items.len());

            let mut attempt_no = 0;
            loop {
                match self.app.logs_repo.upload(items.as_slice()).await {
                    Ok(_) => {
                        println!("Events are updated to db");
                        break;
                    }
                    Err(err) => {
                        println!("Failed to upload logs to db: {:?}", err);
                        attempt_no += 1;
                        if attempt_no > 3 {
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl Into<LogItemDto> for LogItem {
    fn into(self) -> LogItemDto {
        let mut context = BTreeMap::new();

        for itm in self.ctx {
            context.insert(itm.key, itm.value);
        }
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
            context,
        }
    }
}
