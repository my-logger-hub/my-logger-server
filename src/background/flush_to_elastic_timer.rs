use std::sync::Arc;

use elastic_client::{ElasticClient, ElasticIndexRotationPattern};
use rust_extensions::MyTimerTick;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::app::AppContext;

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

#[async_trait::async_trait]
impl MyTimerTick for FlushToElastic {
    async fn tick(&self) {
        let Some(elastic_client) = self.app.elastic_client.as_ref() else {
            return;
        };

        while let Some(items) = self.app.logs_queue.get(1000).await {
            let data_to_upload = items
                .into_iter()
                .map(|x| {
                    let mut model = serde_json::to_value(ElasticLogModel {
                        _id: x.id,
                        env_source: x.tenant.to_uppercase(),
                        log_level: x.level.to_string().to_string(),
                        process: x.process.unwrap_or("N/A".to_string()),
                        message: x.message,
                        date: x.timestamp.unix_microseconds / 1000,
                        app: x.ctx.get(APP_CONTEXT).cloned().unwrap_or("N/A".to_string()),
                    })
                    .unwrap();

                    if let serde_json::Value::Object(ref mut map) = model {
                        for (key, value) in x.ctx {
                            if key != APP_CONTEXT {
                                map.insert(key, Value::String(value));
                            }
                        }
                    }

                    model
                })
                .collect::<Vec<_>>();

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

            elastic_client
                .write_entities(index_name, &pattern, data_to_upload)
                .await
                .unwrap();
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
