use std::{collections::BTreeMap, sync::Arc};

use rust_extensions::{date_time::DateTimeAsMicroseconds, str_utils::StrUtils, MyTimerTick};

use crate::{app::AppContext, repo::DateKey};

pub struct GcTimer {
    pub app: Arc<AppContext>,
    pub tenant: String,
}

impl GcTimer {
    pub async fn new(app: Arc<AppContext>) -> Self {
        let tenant = app.settings_reader.get_default_tenant().await;
        Self { app, tenant }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for GcTimer {
    async fn tick(&self) {
        let mut to_date = DateTimeAsMicroseconds::now();
        to_date.add_days(-1);

        self.app.logs_repo.gc(to_date).await;

        let mut to_date = DateTimeAsMicroseconds::now();
        to_date.add_minutes(-10);
        self.app
            .logs_repo
            .gc_level(&self.tenant, to_date, crate::repo::dto::LogLevelDto::Debug)
            .await;
        self.app
            .logs_repo
            .gc_level(&self.tenant, to_date, crate::repo::dto::LogLevelDto::Info)
            .await;

        let mut to_date = DateTimeAsMicroseconds::now();
        to_date.add_hours(-6);

        self.app
            .logs_repo
            .gc_level(
                &self.tenant,
                to_date,
                crate::repo::dto::LogLevelDto::Warning,
            )
            .await;

        gc_files(&self.app).await;
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

    for (date_ke, tenant) in to_gc {
        println!(
            "File would be GC: {:?} with date_key {}",
            tenant,
            date_ke.get_value()
        );
    }
}

fn check_if_file_name_with_logs(file_name: &str) -> Option<(String, DateKey)> {
    let (left_element, right_element) = file_name.split_exact_to_2_lines("-")?;

    let date_component = DateKey::parse_from_str(right_element)?;

    return (left_element.to_string(), date_component).into();
}
