use serde::{Deserialize, Serialize};
#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    #[serde(rename = "DefaultTenant")]
    pub default_tenant: String,

    #[serde(rename = "ApiKey")]
    pub api_key: Option<String>,

    #[serde(rename = "PostgresConnString")]
    pub postgres_conn_string: String,
}

impl SettingsReader {
    /*
    pub async fn get_api_key(&self) -> Option<String> {
        let read_access = self.settings.read().await;
        read_access.api_key.clone()
    }
     */

    pub async fn get_default_tenant(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.default_tenant.clone()
    }
}

#[async_trait::async_trait]
impl my_postgres::PostgresSettings for SettingsReader {
    async fn get_connection_string(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.postgres_conn_string.clone()
    }
}
