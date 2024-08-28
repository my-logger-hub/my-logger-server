use std::{os::unix::process, sync::Arc};

use elastic_client::{ElasticClient, ElasticIndexRotationPattern};
use rust_extensions::MyTimerTick;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::app::{AppContext, LogItem};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ElasticLogModel {
    pub _id: String,
    pub date: i64,
    pub app: String,
    pub log_level: String,
    pub message: String,
    pub env_source: String,
    pub process: String,
}

impl ElasticLogModel {
    pub fn from_log_into_to_json_value(value: &LogItem) -> serde_json::Value {
        if let Some(process) = &value.process {
            if process.contains(AUTO_PANIC_HANDLER) {
                let mut model = serde_json::to_value(ElasticLogModel {
                    _id: value.id.clone(),
                    env_source: value.tenant.to_uppercase(),
                    log_level: value.level.to_string().to_string(),
                    process: AUTO_PANIC_HANDLER.to_string(),
                    message: "Auto panic handler".to_string(),
                    date: value.timestamp.unix_microseconds / 1000,
                    app: value
                        .ctx
                        .get(APP_CONTEXT)
                        .cloned()
                        .unwrap_or("N/A".to_string()),
                })
                .unwrap();

                if let serde_json::Value::Object(ref mut map) = model {
                    for (key, value) in value.ctx.clone() {
                        if key != APP_CONTEXT {
                            map.insert(key, Value::String(value));
                        }
                    }

                    map.insert(
                        "Panic Message".to_string(),
                        Value::String(value.message.clone()),
                    );
                }

                return model;
            }
        }

        let mut model = serde_json::to_value(ElasticLogModel {
            _id: value.id.clone(),
            env_source: value.tenant.to_uppercase(),
            log_level: value.level.to_string().to_string(),
            process: value.process.clone().unwrap_or("N/A".to_string()),
            message: value.message.clone(),
            date: value.timestamp.unix_microseconds / 1000,
            app: value
                .ctx
                .get(APP_CONTEXT)
                .cloned()
                .unwrap_or("N/A".to_string()),
        })
        .unwrap();

        if let serde_json::Value::Object(ref mut map) = model {
            for (key, value) in value.ctx.clone() {
                if key != APP_CONTEXT {
                    map.insert(key, Value::String(value));
                }
            }
        }

        model
    }
}

pub struct FlushToElastic {
    pub app: Arc<AppContext>,
    last_created_index: Mutex<Option<String>>,
}

impl FlushToElastic {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self {
            app,
            last_created_index: Mutex::new(None),
        }
    }
}

const APP_CONTEXT: &str = "Application";
const AUTO_PANIC_HANDLER: &str = "Panic Handler";

#[async_trait::async_trait]
impl MyTimerTick for FlushToElastic {
    async fn tick(&self) {
        let Some(elastic_client) = self.app.elastic_client.as_ref() else {
            println!("Elastic client is not initialized");
            return;
        };

        while let Some(items) = self.app.logs_queue.get_elastic(1000).await {
            let index_name = "services_logs";
            let pattern = ElasticIndexRotationPattern::Day;

            let mut index = self.last_created_index.lock().await;

            let current_date_index =
                elastic_client.get_index_name_with_pattern(index_name, &pattern);

            if index.is_none() {
                *index = Some(current_date_index.clone());
                init_elastic_log_index(elastic_client, index_name, &pattern).await;
            }

            if index.clone().unwrap() != current_date_index {
                *index = Some(current_date_index);
                init_elastic_log_index(elastic_client, index_name, &pattern).await;
            }

            let response = elastic_client
                .write_entities(index_name, &pattern, items.into_iter().collect())
                .await
                .unwrap();

            println!(
                "Elastic write StatusCode: {};\nResponse: {:#?}",
                response.status_code(),
                response
            );
        }
    }
}

async fn init_elastic_log_index(
    elastic: &ElasticClient,
    index_name: &str,
    index_pattern: &ElasticIndexRotationPattern,
) {
    let mapping = json!({
        "mappings": {
            "properties": {
                "date": { "type": "date", "format": "epoch_millis" },
                "app": { "type": "keyword" },
                "log_level": { "type": "keyword" },
                "message": { "type": "keyword" },
                "env_source": { "type": "keyword" },
                "process": { "type": "keyword" },
            }
        }
    });

    let response = elastic
        .create_index_mapping(index_name, index_pattern, mapping)
        .await;

    println!("Create index response: {:#?}", response);
}
