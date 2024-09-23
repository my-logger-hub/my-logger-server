use std::collections::{BTreeMap, HashMap};

use tokio::sync::Mutex;

pub struct InsightsRepo {
    data: Mutex<HashMap<String, Vec<String>>>,
    max_value_size: usize,
}

impl InsightsRepo {
    pub fn new(keys: Vec<String>, max_value_size: usize) -> Self {
        let mut data = HashMap::new();

        for key in keys {
            data.insert(key, Vec::new());
        }

        InsightsRepo {
            data: Mutex::new(data),
            max_value_size,
        }
    }

    pub async fn get_keys(&self) -> Vec<String> {
        let data = self.data.lock().await;
        data.keys().cloned().collect()
    }

    pub async fn try_insert_values(&self, key_value: &BTreeMap<String, String>) {
        let mut data = self.data.lock().await;

        for (key, value) in key_value {
            if value.len() > self.max_value_size {
                continue;
            }
            if let Some(values) = data.get_mut(key) {
                if values.contains(value) {
                    continue;
                }

                values.push(value.to_string());
            }
        }
    }

    pub async fn get_values(
        &self,
        key: &str,
        phrase: &str,
        max_result_items: usize,
    ) -> Vec<String> {
        let mut result = Vec::new();
        let data = self.data.lock().await;

        if let Some(values) = data.get(key) {
            for value in values {
                if value.contains(phrase) {
                    result.push(value.to_string());

                    if result.len() >= max_result_items {
                        break;
                    }
                }
            }
        }

        result
    }
}
