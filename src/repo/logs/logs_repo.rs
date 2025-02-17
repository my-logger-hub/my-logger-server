use std::{collections::BTreeMap, sync::Arc, time::Duration};

use rust_extensions::{date_time::DateTimeAsMicroseconds, file_utils::FilePath};
use tokio::sync::Mutex;

use super::{LogEventFileGrpcModel, TenMinKey, TenMinLog};

pub struct LogsRepo {
    logs_db_path: FilePath,
    files: Mutex<BTreeMap<TenMinKey, Arc<Mutex<TenMinLog>>>>,
}

impl LogsRepo {
    pub fn new(logs_db_path: FilePath) -> Self {
        Self {
            files: Mutex::default(),
            logs_db_path,
        }
    }

    async fn get_ten_min_file_to_upload(&self, ten_min_key: TenMinKey) -> Arc<Mutex<TenMinLog>> {
        let mut files = self.files.lock().await;

        if let Some(item) = files.get(&ten_min_key) {
            return item.clone();
        }

        let file_name = super::file_utils::compile_file_name(&self.logs_db_path, ten_min_key);

        let file = TenMinLog::open_or_create(&file_name).await;

        let file = Arc::new(Mutex::new(file));

        files.insert(ten_min_key, file.clone());

        file
    }

    async fn get_ten_min_file_to_read(
        &self,
        ten_min_key: TenMinKey,
    ) -> Option<Arc<Mutex<TenMinLog>>> {
        let files = self.files.lock().await;

        if let Some(item) = files.get(&ten_min_key) {
            return Some(item.clone());
        }

        drop(files);

        let file_path = super::file_utils::compile_file_name(&self.logs_db_path, ten_min_key);

        let result = TenMinLog::open(&file_path).await?;

        Some(Arc::new(Mutex::new(result)))
    }

    pub async fn upload_logs(&self, ten_min_key: TenMinKey, events: &[LogEventFileGrpcModel]) {
        let ten_min_log = self.get_ten_min_file_to_upload(ten_min_key).await;
        let mut write_access = ten_min_log.lock().await;
        write_access.upload_logs(events).await;
    }

    pub async fn scan(
        &self,
        from_date: DateTimeAsMicroseconds,
        to_date: DateTimeAsMicroseconds,
        take: usize,
        filter: &impl Fn(&LogEventFileGrpcModel) -> Option<bool>,
    ) -> Vec<LogEventFileGrpcModel> {
        let mut result = Vec::new();

        let mut key: TenMinKey = from_date.into();

        let to_key: TenMinKey = to_date.into();

        while key.as_u64() < to_key.as_u64() {
            if let Some(file) = self.get_ten_min_file_to_read(key).await {
                let mut file_access = file.lock().await;

                let iterator = file_access.iter().await;

                while let Some(item) = iterator.get_next().await {
                    match filter(&item) {
                        Some(insert_it) => {
                            if insert_it {
                                result.push(item);

                                if result.len() >= take {
                                    return result;
                                }
                            }
                        }
                        None => return result,
                    }
                }
            }

            key.inc();
        }

        result
    }

    pub async fn gc(&self, now: DateTimeAsMicroseconds, duration_to_gc: Duration) {
        let delete_from = now.sub(duration_to_gc);
        super::file_utils::gc_files(&self.logs_db_path, delete_from).await;
    }
}
