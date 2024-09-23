use std::sync::Arc;

use rust_extensions::{date_time::DateTimeAsMicroseconds, MyTimerTick};

use crate::{
    app::AppContext,
    repo::{DateHourKey, LOG_FILE_PREFIX},
};

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
        {
            let mut hourly_statistics = self.app.hourly_statistics.lock().await;
            hourly_statistics.gc();
        }

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
    let gc_date_key: DateHourKey = gc_date_key.into();

    println!("GC from date_key: {}", gc_date_key.get_value());

    let mut to_gc = Vec::new();
    for file_name in files {
        let file_to_process = check_if_file_name_with_logs(&file_name);
        if file_to_process.is_none() {
            continue;
        }

        let file_to_process = file_to_process;

        if file_to_process.is_none() {
            continue;
        }

        let date_key = file_to_process.unwrap();

        if date_key <= gc_date_key {
            to_gc.push(date_key);
        }
    }

    for date_key in to_gc {
        let file_name = app.logs_repo.compile_file_name(date_key);
        app.logs_repo.prepare_to_delete(date_key).await;

        let result = tokio::fs::remove_file(file_name.as_str()).await;

        if let Err(err) = result {
            panic!("Can not delete file {}. Err: {}", file_name, err);
        }
    }
}

fn check_if_file_name_with_logs(file_name: &str) -> Option<DateHourKey> {
    if !file_name.starts_with(LOG_FILE_PREFIX) {
        return None;
    }

    let date_key_as_string = &file_name[LOG_FILE_PREFIX.len()..];

    let date_component = DateHourKey::parse_from_str(date_key_as_string);

    if date_component.is_none() {
        println!(
            "Somehow file {} has wrong date_key_as_string='{}'",
            file_name, date_key_as_string
        );
    }

    return date_component;
}
