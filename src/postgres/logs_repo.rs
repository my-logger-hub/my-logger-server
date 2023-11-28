use std::sync::Arc;

use my_postgres::{MyPostgres, MyPostgresError, PostgresSettings};
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::app::{LogCtxItem, APP_NAME};

use super::dto::*;

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

    pub async fn get(
        &self,
        tenant: &str,
        from_date: DateTimeAsMicroseconds,
        to_date: Option<DateTimeAsMicroseconds>,
        levels: Option<Vec<LogLevelDto>>,
        context: Option<Vec<LogCtxItem>>,
        take: usize,
    ) -> Result<Vec<LogItemDto>, MyPostgresError> {
        let where_model = WhereModel {
            tenant,
            from_date,
            to_date,
            level: levels,
            take,
        };

        self.postgres
            .query_rows(TABLE_NAME, Some(&where_model))
            .await
    }

    pub async fn get_statistics(
        &self,
        tenant: &str,
        from_date: DateTimeAsMicroseconds,
        to_date: Option<DateTimeAsMicroseconds>,
    ) -> Result<Vec<StatisticsModel>, MyPostgresError> {
        let where_model = WhereStatisticsModel {
            tenant,
            from_date,
            to_date,
            level: None,
        };

        self.postgres
            .query_rows(TABLE_NAME, Some(&where_model))
            .await
    }
}
