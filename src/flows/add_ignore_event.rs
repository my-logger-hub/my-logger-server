use std::collections::BTreeMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{
    app::{AppContext, LogItem},
    repo::dto::IgnoreItemDto,
};

pub async fn add_ignore_event(app: &AppContext, event: IgnoreItemDto) {
    app.settings_repo
        .add_ignore_event(&IgnoreItemDto {
            level: event.level.clone().into(),
            application: event.application.to_string(),
            marker: event.marker.to_string(),
        })
        .await;

    app.filter_events_cache.reset().await;

    let telegram_settings = app.settings_reader.get_telegram_settings().await;

    if let Some(telegram_settings) = telegram_settings {
        let dt = DateTimeAsMicroseconds::now();

        let mut ctx = BTreeMap::new();
        ctx.insert("Level".to_string(), format!("{:?}", &event.level));
        ctx.insert("Application".to_string(), event.application);
        ctx.insert("Marker".to_string(), event.marker);

        crate::telegram::api::send_log_item(
            &telegram_settings,
            &LogItem {
                id: dt.to_rfc3339(),
                level: my_logger::LogLevel::Info,
                process: None,
                message: "Ignore event is added".to_string(),
                timestamp: dt,
                ctx,
            },
            &app.env_name,
        )
        .await;
    }
}
