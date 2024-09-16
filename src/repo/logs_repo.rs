use std::{collections::BTreeMap, sync::Arc, time::Duration};

use my_sqlite::*;
use rust_extensions::date_time::DateTimeAsMicroseconds;
use sql::RawField;
use sql_where::NoneWhereModel;
use tokio::sync::Mutex;

use super::{dto::*, DateKey};

const TABLE_NAME: &str = "logs";
//const PK_NAME: &str = "logs_pk";

//const MAX_POOL_SIZE: usize = 10;

#[derive(Default)]
pub struct LogsRepoPool {
    pool: BTreeMap<DateKey, Arc<SqlLiteConnection>>,
    to_delete: Option<DateKey>,
}

impl LogsRepoPool {
    async fn get_or_create_sqlite(
        &mut self,
        date_key: DateKey,
        get_file_name: impl Fn() -> String,
    ) -> Arc<SqlLiteConnection> {
        if let Some(result) = self.pool.get(&date_key) {
            return result.clone();
        }

        let path = get_file_name();

        println!("Creating new instance: {}", path);

        let sqlite = SqlLiteConnectionBuilder::new(path)
            .create_table_if_no_exists::<LogItemDto>(TABLE_NAME)
            .build()
            .await
            .unwrap();

        let sqlite = Arc::new(sqlite);

        self.pool.insert(date_key, sqlite.clone());

        sqlite
    }

    async fn get_sqlite(
        &mut self,
        date_key: DateKey,
        get_file_name: impl Fn() -> String,
    ) -> Option<Arc<SqlLiteConnection>> {
        if let Some(to_delete) = self.to_delete.as_ref() {
            if to_delete.get_value() == date_key.get_value() {
                return None;
            }
        }

        if let Some(instance) = self.pool.get(&date_key) {
            return Some(instance.clone());
        }

        let file = get_file_name();

        {
            if tokio::fs::metadata(file.as_str()).await.is_err() {
                return None;
            }
        }

        println!("Opening existing instance: {}", file);
        let sqlite = SqlLiteConnectionBuilder::new(file)
            .create_table_if_no_exists::<LogItemDto>(TABLE_NAME)
            .build()
            .await
            .unwrap();

        let sqlite = Arc::new(sqlite);
        self.pool.insert(date_key, sqlite.clone());

        Some(sqlite)
    }
}

pub struct LogsRepo {
    sqlite_pool: Mutex<LogsRepoPool>,
    path: String,
}

impl LogsRepo {
    pub async fn new(mut path: String) -> Self {
        if path.chars().last().unwrap() != std::path::MAIN_SEPARATOR {
            path.push(std::path::MAIN_SEPARATOR);
        }

        Self {
            sqlite_pool: Mutex::new(LogsRepoPool::default()),
            path,
        }
    }

    pub fn compile_file_name(&self, date_key: DateKey) -> String {
        let mut path = self.path.clone();

        path.push_str("logs-");
        path.push_str(date_key.get_value().to_string().as_str());
        path.push_str(".db");
        path
    }

    async fn get_or_create_sqlite(&self, date_key: DateKey) -> Arc<SqlLiteConnection> {
        let mut write_access = self.sqlite_pool.lock().await;

        write_access
            .get_or_create_sqlite(date_key, || self.compile_file_name(date_key))
            .await
    }

    async fn get_sqlite(&self, date_key: DateKey) -> Option<Arc<SqlLiteConnection>> {
        let mut write_access = self.sqlite_pool.lock().await;

        write_access
            .get_sqlite(date_key, || self.compile_file_name(date_key))
            .await
    }

    async fn get_last_sqlite(&self) -> Option<Arc<SqlLiteConnection>> {
        let now = DateTimeAsMicroseconds::now();
        let date_key: DateKey = now.into();
        self.get_sqlite(date_key).await
    }

    async fn get_last_and_before(&self) -> Vec<Arc<SqlLiteConnection>> {
        let now = DateTimeAsMicroseconds::now();
        let date_key_now: DateKey = now.into();

        let before = now.sub(Duration::from_secs(60 * 60));

        let date_key_before: DateKey = before.into();

        let mut result = Vec::with_capacity(2);

        let mut write_access = self.sqlite_pool.lock().await;

        if let Some(sqlite) = write_access
            .get_sqlite(date_key_now, || self.compile_file_name(date_key_now))
            .await
        {
            result.push(sqlite)
        }

        if let Some(sqlite) = write_access
            .get_sqlite(date_key_before, || self.compile_file_name(date_key_before))
            .await
        {
            result.push(sqlite)
        }

        return result;
    }

    pub async fn upload(&self, date_key: DateKey, items: &[LogItemDto]) {
        self.get_or_create_sqlite(date_key)
            .await
            .bulk_insert_db_entities_if_not_exists(items, TABLE_NAME)
            .await
            .unwrap();
    }

    pub async fn get(
        &self,
        from_date: DateTimeAsMicroseconds,
        to_date: Option<DateTimeAsMicroseconds>,
        levels: Option<Vec<LogLevelDto>>,
        context: Option<BTreeMap<String, String>>,
        take: usize,
    ) -> Vec<LogItemDto> {
        let where_model = WhereModel {
            from_date,
            to_date,
            level: levels,
            take,
            context,
        };

        let files = DateKey::get_keys_to_request(
            from_date,
            to_date.unwrap_or(DateTimeAsMicroseconds::now()),
        );

        println!("Files to request: {:?}", files);

        let mut result = Vec::new();

        for date_key in files.keys().rev() {
            let sqlite = self.get_sqlite(*date_key).await;
            if let Some(sqlite) = sqlite {
                let items = sqlite.query_rows(TABLE_NAME, Some(&where_model)).await;

                match items {
                    Ok(items) => {
                        result.extend(items);
                    }
                    Err(e) => {
                        println!("Error: {:?}", e);
                    }
                }

                if result.len() >= take {
                    break;
                }
            }
        }

        result
    }

    pub async fn scan(
        &self,
        tenant: &str,
        from_date: DateTimeAsMicroseconds,
        to_date: DateTimeAsMicroseconds,
        phrase: &str,
        limit: usize,
        debug: bool,
    ) -> Vec<LogItemDto> {
        let where_model = WhereScanModel {
            from_date: from_date.to_rfc3339()[..19].to_string(),
            to_date: to_date.to_rfc3339()[..19].to_string(),
            phrase: RawField {
                value: phrase.to_string(),
            },
            limit,
        };

        let files = DateKey::get_keys_to_request(from_date, to_date);

        let mut result = Vec::new();

        for date_key in files.keys().rev() {
            if debug {
                println!(
                    "Requesting from tenant:'{tenant}' file:'{}'. From: {}, to: {}",
                    date_key.get_value(),
                    from_date.to_rfc3339(),
                    to_date.to_rfc3339()
                );
            }
            let sqlite = self.get_sqlite(*date_key).await;
            if let Some(sqlite) = sqlite {
                let items = sqlite.query_rows(TABLE_NAME, Some(&where_model)).await;

                match items {
                    Ok(items) => {
                        result.extend(items);
                    }
                    Err(e) => {
                        println!("Error: {:?}", e);
                    }
                }

                if result.len() >= limit {
                    break;
                }
            }
        }

        result
    }

    pub async fn prepare_to_delete(&self, date_key: DateKey) {
        let mut write_access = self.sqlite_pool.lock().await;

        write_access.to_delete = Some(date_key);
    }

    pub async fn get_statistics(&self) -> Vec<StatisticsModel> {
        if let Some(sqlite) = self.get_last_sqlite().await {
            return sqlite
                .query_rows(TABLE_NAME, NoneWhereModel::new())
                .await
                .unwrap();
        }

        Vec::new()
    }

    pub async fn get_files(&self) -> Vec<String> {
        let mut files = Vec::new();

        {
            let mut read_dir = tokio::fs::read_dir(self.path.as_str()).await.unwrap();

            while let Some(dir_entry) = read_dir.next_entry().await.unwrap() {
                let file_type = dir_entry.file_type().await.unwrap();

                if file_type.is_file() {
                    files.push(dir_entry.file_name().into_string().unwrap());
                }
            }
        }

        files
    }

    pub async fn gc(&self, to_date: DateTimeAsMicroseconds) -> Vec<DateKey> {
        let gc_from = DateKey::new(to_date);

        let mut read_access = self.sqlite_pool.lock().await;

        let mut to_gc = Vec::new();
        for date_key in read_access.pool.keys() {
            if date_key < &gc_from {
                to_gc.push(*date_key)
            }
        }

        for to_gc in &to_gc {
            read_access.pool.remove(to_gc);
        }

        to_gc
    }

    pub async fn gc_level(&self, to_date: DateTimeAsMicroseconds, level: LogLevelDto) {
        let where_model = DeleteLevelWhereModel {
            moment: to_date,
            level,
        };

        let last_and_before = self.get_last_and_before().await;

        for itm in last_and_before {
            itm.delete_db_entity(TABLE_NAME, &where_model)
                .await
                .unwrap();
        }
    }
}
