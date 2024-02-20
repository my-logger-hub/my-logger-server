use serde::{Deserialize, Serialize};

use crate::app::LogItem;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IgnoreEvent {
    pub level: String,
    pub application: String,
    pub marker: String,
}

impl IgnoreEvent {
    pub fn matches_ignore_filter(&self, log_event: &LogItem) -> bool {
        if !log_event.is_level(&self.level) {
            return false;
        }

        if !log_event.is_application(&self.application) {
            return false;
        }

        log_event.has_entry(self.marker.as_str())
    }
}

#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    #[serde(rename = "DefaultTenant")]
    pub default_tenant: String,

    #[serde(rename = "ApiKey")]
    pub api_key: Option<String>,

    #[serde(rename = "LogsDbPath")]
    pub logs_db_path: String,

    #[serde(rename = "IgnoreEvents")]
    pub ignore: Option<Vec<IgnoreEvent>>,
}

impl SettingsReader {
    pub async fn filter_events<T>(
        &self,
        events: Vec<T>,
        filter: impl Fn(&T, &[IgnoreEvent]) -> bool,
    ) -> Vec<T> {
        let read_access = self.settings.read().await;

        if read_access.ignore.is_none() {
            return events;
        }

        let mut result = Vec::with_capacity(events.len());

        for itm in events {
            if filter(&itm, read_access.ignore.as_ref().unwrap()) {
                result.push(itm);
            }
        }

        result
    }

    pub async fn get_logs_db_path(&self, file_name: &str) -> String {
        let read_access = self.settings.read().await;

        let mut result = if read_access.logs_db_path.starts_with("~") {
            read_access
                .logs_db_path
                .replace("~", &std::env::var("HOME").unwrap())
        } else {
            read_access.logs_db_path.clone()
        };

        if !result.ends_with(std::path::MAIN_SEPARATOR) {
            result.push('/')
        }

        result.push_str(file_name);

        result
    }

    pub async fn get_default_tenant(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.default_tenant.clone()
    }
}
