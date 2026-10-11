use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TelegramSettings {
    pub api_key: String,
    pub chat_id: i64,
    pub message_thread_id: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ElasticSettings {
    pub esecure: String,
    pub url: String,
    pub env_source: String,
}

/// Where the Warning, Error and FatalError hours go 2 hours after they end, to be kept for
/// `hours_to_gc` of `settings.json`. Without it they are deleted then.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct S3Settings {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    /// The folder of the hours in the bucket; `logs` when it is not set
    pub prefix: Option<String>,
}

#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    pub logs_db_path: String,

    pub telegram_settings: Option<TelegramSettings>,


    pub elastic: Option<ElasticSettings>,


    pub s3: Option<S3Settings>,

    pub env_name: String,
}

impl SettingsReader {
    pub async fn get_telegram_settings(&self) -> Option<TelegramSettings> {
        let read_access = self.settings.read().await;
        read_access.telegram_settings.clone()
    }

    pub async fn get_env_name(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.env_name.clone()
    }

    pub async fn get_elastic_settings(&self) -> Option<ElasticSettings> {
        let read_access = self.settings.read().await;
        read_access.elastic.clone()
    }

    pub async fn get_s3_settings(&self) -> Option<S3Settings> {
        let read_access = self.settings.read().await;
        read_access.s3.clone()
    }

    pub async fn get_logs_db_path(&self, file_name: Option<&str>) -> String {
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

        if let Some(file_name) = file_name {
            result.push_str(file_name);
        }

        result
    }
}
