use std::collections::BTreeMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{
    app::{AppContext, LogItem},
    repo::dto::IgnoreWhereModel,
};

pub async fn remove_ignore_event(app: &AppContext, event: IgnoreWhereModel) {
    app.settings_repo
        .delete_ignore_event(&IgnoreWhereModel {
            level: event.level.clone().into(),
            application: event.application.clone(),
            marker: event.marker.clone(),
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

        crate::telegram_api::send_log_item(
            &telegram_settings,
            &LogItem {
                id: dt.to_rfc3339(),
                tenant: "System".to_string(),
                level: my_logger::LogLevel::Info,
                process: None,
                message: "Ignore event removed".to_string(),
                timestamp: dt,
                ctx,
            },
        )
        .await;
    }
}
