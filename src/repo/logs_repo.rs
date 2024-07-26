use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use my_sqlite::*;
use rust_extensions::date_time::DateTimeAsMicroseconds;
use sql_where::NoneWhereModel;
use tokio::sync::Mutex;

use super::{dto::*, DateKey};

const TABLE_NAME: &str = "logs";
//const PK_NAME: &str = "logs_pk";

//const MAX_POOL_SIZE: usize = 10;

pub struct LogsRepo {
    sqlite_pool: Mutex<HashMap<String, BTreeMap<DateKey, Arc<SqlLiteConnection>>>>,
    path: String,
}

impl LogsRepo {
    pub async fn new(mut path: String) -> Self {
        if path.chars().last().unwrap() != std::path::MAIN_SEPARATOR {
            path.push(std::path::MAIN_SEPARATOR);
        }

        Self {
            sqlite_pool: Mutex::new(HashMap::new()),
            path,
        }
    }

    fn compile_file_name(&self, tenant: &str, date_key: DateKey) -> String {
        let mut path = self.path.clone();
        path.push_str(&tenant);
        path.push('-');
        path.push_str(date_key.get_value().to_string().as_str());
        path
    }

    async fn get_or_create_sqlite(
        &self,
        tenant: &str,
        date_key: DateKey,
    ) -> Arc<SqlLiteConnection> {
        let mut pool = self.sqlite_pool.lock().await;

        if !pool.contains_key(tenant) {
            pool.insert(tenant.to_string(), BTreeMap::new());
        }

        let pool_by_tenant = pool.get_mut(tenant).unwrap();

        if let Some(result) = pool_by_tenant.get(&date_key) {
            return result.clone();
        }

        let path = self.compile_file_name(tenant, date_key);

        println!("Creating new instance: {}", path);

        let sqlite = SqlLiteConnectionBuilder::new(path)
            .create_table_if_no_exists::<LogItemDto>(TABLE_NAME)
            .build()
            .await
            .unwrap();

        let sqlite = Arc::new(sqlite);

        pool_by_tenant.insert(date_key, sqlite.clone());

        sqlite
    }

    async fn get_sqlite(&self, tenant: &str, date_key: DateKey) -> Option<Arc<SqlLiteConnection>> {
        let mut pool = self.sqlite_pool.lock().await;

        let by_tenant = pool.get_mut(tenant)?;
        if let Some(instance) = by_tenant.get(&date_key) {
            return Some(instance.clone());
        }

        let file = self.compile_file_name(tenant, date_key);

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
        by_tenant.insert(date_key, sqlite.clone());

        Some(sqlite)
    }

    async fn get_last_sqlite(&self, tenant: &str) -> Option<Arc<SqlLiteConnection>> {
        let pool = self.sqlite_pool.lock().await;
        let by_tenant = pool.get(tenant)?;
        by_tenant.values().last().cloned()
    }

    async fn get_last_and_before(&self) -> Vec<Arc<SqlLiteConnection>> {
        let mut result = Vec::new();

        let pool = self.sqlite_pool.lock().await;

        for pool in pool.values() {
            let last = pool.values().last().cloned();
            let before = pool.values().nth_back(1).cloned();

            if let Some(last) = last {
                result.push(last);
            }

            if let Some(before) = before {
                result.push(before);
            }
        }

        return result;
    }

    pub async fn upload(&self, tenant: &str, date_key: DateKey, items: &[LogItemDto]) {
        self.get_or_create_sqlite(tenant, date_key)
            .await
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

        let files = DateKey::get_keys_to_request(
            from_date,
            to_date.unwrap_or(DateTimeAsMicroseconds::now()),
        );

        println!("Files to request: {:?}", files);

        let mut result = Vec::new();

        for date_key in files.keys().rev() {
            let sqlite = self.get_sqlite(tenant, *date_key).await;
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

    pub async fn get_statistics(&self, tenant: &str) -> Vec<StatisticsModel> {
        if let Some(sqlite) = self.get_last_sqlite(tenant).await {
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

    pub async fn gc(&self, to_date: DateTimeAsMicroseconds) -> HashMap<String, Vec<DateKey>> {
        let gc_from = DateKey::new(to_date);

        let mut read_access = self.sqlite_pool.lock().await;

        let mut result = HashMap::new();

        for (tenant, pool) in read_access.iter_mut() {
            let mut to_gc = Vec::new();
            for date_key in pool.keys() {
                if date_key < &gc_from {
                    to_gc.push(*date_key)
                }
            }

            for to_gc in &to_gc {
                pool.remove(to_gc);
            }

            result.insert(tenant.to_string(), to_gc);
        }

        result
    }

    pub async fn gc_level(
        &self,
        tenant: &str,
        to_date: DateTimeAsMicroseconds,
        level: LogLevelDto,
    ) {
        let where_model = DeleteLevelWhereModel {
            tenant,
            to_date,
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
