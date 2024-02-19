use serde::{Deserialize, Serialize};
#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    #[serde(rename = "DefaultTenant")]
    pub default_tenant: String,

    #[serde(rename = "ApiKey")]
    pub api_key: Option<String>,

    #[serde(rename = "LogsDbPath")]
    pub logs_db_path: String,
}

impl SettingsReader {
    /*
    pub async fn get_api_key(&self) -> Option<String> {
        let read_access = self.settings.read().await;
        read_access.api_key.clone()
    }
     */

    pub async fn get_logs_db_path(&self, file_name: &str) -> String {
        let read_access = self.settings.read().await;

        let mut result = if read_access.logs_db_path.starts_with("~") {
            return read_access
                .logs_db_path
                .replace("~", &std::env::var("HOME").unwrap());
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

/*
#[async_trait::async_trait]
impl my_postgres::PostgresSettings for SettingsReader {
    async fn get_connection_string(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.postgres_conn_string.clone()
    }
}
 */
