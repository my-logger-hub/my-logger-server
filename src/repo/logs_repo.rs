use std::collections::BTreeMap;

use my_sqlite::*;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use super::dto::*;

const TABLE_NAME: &str = "logs";
//const PK_NAME: &str = "logs_pk";

pub struct LogsRepo {
    sqlite: SqlLiteConnection,
}

impl LogsRepo {
    pub async fn new(path: String) -> Self {
        Self {
            sqlite: SqlLiteConnectionBuilder::new(path)
                .create_table_if_no_exists::<LogItemDto>(TABLE_NAME)
                .build()
                .await
                .unwrap(),
        }
    }

    pub async fn upload(&self, items: &[LogItemDto]) {
        self.sqlite
            .bulk_insert_db_entities_if_not_exists(items, TABLE_NAME)
            .await
            .unwrap();
    }

    pub async fn get(
        &self,
        tenant: &str,
        from_date: DateTimeAsMicroseconds,
        to_date: Option<DateTimeAsMicroseconds>,
        levels: Option<Vec<LogLevelDto>>,
        context: Option<BTreeMap<String, String>>,
        take: usize,
    ) -> Vec<LogItemDto> {
        let where_model = WhereModel {
            tenant,
            from_date,
            to_date,
            level: levels,
            take,
            context,
        };

        let result = self
            .sqlite
            .query_rows(TABLE_NAME, Some(&where_model))
            .await
            .unwrap();

        println!("Got {} records for tenant {}", result.len(), tenant);
        result
    }

    pub async fn get_statistics(
        &self,
        tenant: &str,
        from_date: DateTimeAsMicroseconds,
        to_date: Option<DateTimeAsMicroseconds>,
    ) -> Vec<StatisticsModel> {
        let where_model = WhereStatisticsModel {
            tenant,
            from_date,
            to_date,
            level: None,
        };

        self.sqlite
            .query_rows(TABLE_NAME, Some(&where_model))
            .await
            .unwrap()
    }
}
