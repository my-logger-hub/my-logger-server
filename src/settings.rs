use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TelegramSettings {
    pub api_key: String,
    pub chat_id: i64,
    pub message_thread_id: i32,
    pub env_info: String,
}

#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    #[serde(rename = "DefaultTenant")]
    pub default_tenant: String,

    #[serde(rename = "ApiKey")]
    pub api_key: Option<String>,

    #[serde(rename = "LogsDbPath")]
    pub logs_db_path: String,

    #[serde(rename = "TelegramSettings")]
    pub telegram_settings: Option<TelegramSettings>,
}

impl SettingsReader {
    pub async fn get_telegram_settings(&self) -> Option<TelegramSettings> {
        let read_access = self.settings.read().await;
        read_access.telegram_settings.clone()
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
