use std::collections::BTreeMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::AppContext, log_item::*, repo::ignore_events::IgnoreEventModel};

pub async fn remove(app: &AppContext, event: IgnoreEventModel) {
    app.ignore_events_repo.remove(&event).await;

    let telegram_settings = app.settings_reader.get_telegram_settings().await;

    if let Some(telegram_settings) = telegram_settings {
        let dt = DateTimeAsMicroseconds::now();

        let mut ctx = BTreeMap::new();
        ctx.insert("Level".to_string(), format!("{:?}", &event.level));
        ctx.insert("Marker".to_string(), event.marker);

        crate::telegram::api::send_log_item(
            &telegram_settings,
            &LogEvent {
                id: dt.to_rfc3339(),
                level: my_logger::LogLevel::Info,
                process: None,
                application: event.application.into(),
                message: "Ignore event removed".to_string(),
                timestamp: dt,
                ctx,
            },
            &app.env_name,
        )
        .await;
    }
}
