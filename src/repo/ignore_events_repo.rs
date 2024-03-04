use my_sqlite::{sql_where::NoneWhereModel, SqlLiteConnection, SqlLiteConnectionBuilder};

use super::dto::*;

const TABLE_NAME: &str = "ignore_events";

pub struct SettingsRepo {
    sqlite: SqlLiteConnection,
}

impl SettingsRepo {
    pub async fn new(path: String) -> Self {
        Self {
            sqlite: SqlLiteConnectionBuilder::new(path)
                .create_table_if_no_exists::<IgnoreItemDto>(TABLE_NAME)
                .build()
                .await
                .unwrap(),
        }
    }

    pub async fn get(&self) -> Vec<IgnoreItemDto> {
        self.sqlite
            .query_rows(TABLE_NAME, Some(&NoneWhereModel))
            .await
            .unwrap()
    }

    pub async fn add_ignore_event(&self, item: &IgnoreItemDto) {
        self.sqlite
            .insert_db_entity_if_not_exists(item, TABLE_NAME)
            .await
            .unwrap();
    }
}
