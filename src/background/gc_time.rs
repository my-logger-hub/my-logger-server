use std::{collections::BTreeMap, sync::Arc};

use rust_extensions::{date_time::DateTimeAsMicroseconds, str_utils::StrUtils, MyTimerTick};

use crate::{app::AppContext, repo::DateKey};

pub struct GcTimer {
    pub app: Arc<AppContext>,
}

impl GcTimer {
    pub async fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for GcTimer {
    async fn tick(&self) {
        let mut to_date = DateTimeAsMicroseconds::now();
        to_date.add_days(-1);

        self.app.logs_repo.gc(to_date).await;

        let mut to_date = DateTimeAsMicroseconds::now();
        to_date.add_minutes(-60);
        self.app
            .logs_repo
            .gc_level(to_date, crate::repo::dto::LogLevelDto::Debug)
            .await;
        self.app
            .logs_repo
            .gc_level(to_date, crate::repo::dto::LogLevelDto::Info)
            .await;

        let mut to_date = DateTimeAsMicroseconds::now();
        to_date.add_hours(-6);

        self.app
            .logs_repo
            .gc_level(to_date, crate::repo::dto::LogLevelDto::Warning)
            .await;

        gc_files(&self.app).await;

        self.app.ignore_single_event_cache.lock().await.gc();
    }
}

async fn gc_files(app: &AppContext) {
    let files = app.logs_repo.get_files().await;

    println!("Files: {:#?}", files);

    let gc_from = app.settings_reader.get_duration_to_gc().await;

    let gc_date_key = DateTimeAsMicroseconds::now().sub(gc_from);
    let gc_date_key: DateKey = gc_date_key.into();

    let mut to_gc = BTreeMap::new();
    for file_name in files {
        let file_to_process = check_if_file_name_with_logs(&file_name);
        if file_to_process.is_none() {
            continue;
        }

        let file_to_process = file_to_process;

        if file_to_process.is_none() {
            continue;
        }

        let (tenant, date_key) = file_to_process.unwrap();

        if date_key <= gc_date_key {
            to_gc.insert(date_key, tenant);
        }
    }

    for (date_key, tenant) in to_gc {
        println!(
            "Doing GC for tenant {} with date_key {}",
            tenant,
            date_key.get_value()
        );

        let file_name = app.logs_repo.compile_file_name(&tenant, date_key);
        app.logs_repo.prepare_to_delete(tenant, date_key).await;

        let result = tokio::fs::remove_file(file_name.as_str()).await;

        if let Err(err) = result {
            panic!("Can not delete file {}. Err: {}", file_name, err);
        }
    }
}

fn check_if_file_name_with_logs(file_name: &str) -> Option<(String, DateKey)> {
    let (left_element, right_element) = file_name.split_exact_to_2_lines("-")?;

    let date_component = DateKey::parse_from_str(right_element)?;

    return (left_element.to_string(), date_component).into();
}
