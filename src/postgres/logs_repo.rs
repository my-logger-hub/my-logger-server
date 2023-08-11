use std::sync::Arc;

use my_postgres::{MyPostgres, MyPostgresError, PostgresSettings};

use crate::app::APP_NAME;

use super::dto::LogItemDto;

const TABLE_NAME: &str = "logs";
const PK_NAME: &str = "logs_pk";

pub struct LogsRepo {
    postgres: MyPostgres,
}

impl LogsRepo {
    pub async fn new(settings: Arc<dyn PostgresSettings + Send + Sync + 'static>) -> Self {
        Self {
            postgres: MyPostgres::from_settings(APP_NAME, settings)
                .with_table_schema_verification::<LogItemDto>(TABLE_NAME, Some(PK_NAME.into()))
                .build()
                .await,
        }
    }
    pub async fn upload(&self, items: &[LogItemDto]) -> Result<(), MyPostgresError> {
        self.postgres
            .bulk_insert_db_entities_if_not_exists(TABLE_NAME, items)
            .await
    }
}
