use macros::*;
use my_sqlite::*;

use crate::hourly_statistics::StatisticsHour;

const TABLE_NAME: &str = "h_statistics";
pub struct HourStatisticsRepo {
    sqlite: SqlLiteConnection,
}

impl HourStatisticsRepo {
    pub async fn new(path: String) -> Self {
        Self {
            sqlite: SqlLiteConnectionBuilder::new(path)
                .create_table_if_no_exists::<HourStatisticsDto>(TABLE_NAME)
                .build()
                .await
                .unwrap(),
        }
    }

    pub async fn insert_or_update(&self, dto: &HourStatisticsDto) {
        println!("Inserting or updating {:?}", dto);

        self.sqlite
            .insert_or_update_db_entity(TABLE_NAME, dto)
            .await
            .unwrap();
    }

    pub async fn get_by_keys(&self, date_key: Vec<StatisticsHour>) -> Vec<HourStatisticsDto> {
        let where_model = ByKeysWhereModel {
            date_key: date_key.into_iter().map(|x| x.get_value()).collect(),
        };
        self.sqlite
            .query_rows(TABLE_NAME, Some(&where_model))
            .await
            .unwrap()
    }

    pub async fn get_top_keys(&self, limit: usize) -> Vec<StatisticsHour> {
        let where_model = GetTopKeysDbModel { limit };
        let result: Vec<WhereDbByKeyModel> = self
            .sqlite
            .query_rows(TABLE_NAME, Some(&where_model))
            .await
            .unwrap();

        result.iter().map(|x| x.date_key.into()).collect()
    }
}

#[derive(TableSchema, InsertDbEntity, UpdateDbEntity, SelectDbEntity, Debug)]
pub struct HourStatisticsDto {
    #[primary_key(0)]
    pub date_key: u64,
    #[primary_key(1)]
    pub app: String,
    pub info: u32,
    pub warning: u32,
    pub error: u32,
    pub fatal_error: u32,
    pub debug: u32,
}

#[derive(WhereDbModel)]
pub struct WhereDbModelByDateKey {
    #[limit]
    pub limit: usize,
}

#[derive(WhereDbModel)]
pub struct ByKeysWhereModel {
    pub date_key: Vec<u64>,
}

#[derive(WhereDbModel)]
pub struct GetTopKeysDbModel {
    #[limit]
    pub limit: usize,
}

#[derive(SelectDbEntity)]
pub struct WhereDbByKeyModel {
    #[order_by_desc]
    #[group_by]
    pub date_key: u64,
}
