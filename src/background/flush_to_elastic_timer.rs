use std::sync::Arc;

use elastic_client::{ElasticClient, ElasticIndexRotationPattern};
use rust_extensions::MyTimerTick;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::{app::AppContext, log_item::*};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ElasticLogModel {
    pub inner_id: String,
    pub date: i64,
    pub app: String,
    pub log_level: String,
    pub message: String,
    pub env_source: String,
    pub process: String,
}

impl ElasticLogModel {
    pub fn from_log_into_to_json_value(value: &LogEvent, env_name: &str) -> serde_json::Value {
        if let Some(process) = &value.process {
            if process.contains(AUTO_PANIC_HANDLER) {
                let mut model = serde_json::to_value(ElasticLogModel {
                    inner_id: value.id.clone(),
                    env_source: env_name.to_uppercase(),
                    log_level: value.level.as_str().to_string(),
                    process: AUTO_PANIC_HANDLER.to_string(),
                    message: "Auto panic handler".to_string(),
                    date: value.timestamp.unix_microseconds / 1000,
                    app: value.application.clone().unwrap_or("N/A".to_string()),
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
            inner_id: value.id.clone(),
            env_source: env_name.to_uppercase(),
            log_level: value.level.as_str().to_string(),
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
    pub env_source: String,
    last_created_index: Mutex<Option<String>>,
}

impl FlushToElastic {
    pub fn new(app: Arc<AppContext>, env_source: &str) -> Self {
        Self {
            app,
            last_created_index: Mutex::new(None),
            env_source: env_source.to_string(),
        }
    }
}

const APP_CONTEXT: &str = "Application";
const AUTO_PANIC_HANDLER: &str = "Panic Handler";

#[async_trait::async_trait]
impl MyTimerTick for FlushToElastic {
    async fn tick(&self) {
        let Some(elastic) = self.app.elastic.as_ref() else {
            if self.app.is_debug {
                println!("Elastic client is not initialized");
            }

            return;
        };

        while let Some(items) = elastic.logs_queue.get(1000).await {
            let index_name = &format!("services_logs_{}", self.env_source);
            let pattern = ElasticIndexRotationPattern::Day;

            let mut index = self.last_created_index.lock().await;

            let current_date_index = elastic
                .client
                .get_index_name_with_pattern(index_name, &pattern);

            if index.is_none() {
                *index = Some(current_date_index.clone());
                init_elastic_log_index(&elastic.client, index_name, &pattern).await;
            }

            if index.clone().unwrap() != current_date_index {
                *index = Some(current_date_index);
                init_elastic_log_index(&elastic.client, index_name, &pattern).await;
            }

            let response = elastic
                .client
                .write_entities(
                    index_name,
                    &pattern,
                    items
                        .iter()
                        .map(|itm| {
                            ElasticLogModel::from_log_into_to_json_value(
                                itm.as_ref(),
                                &self.env_source,
                            )
                        })
                        .collect(),
                )
                .await
                .unwrap();

            println!("elastic_status: {}", response.status_code(),);
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
