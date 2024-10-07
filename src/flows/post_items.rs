use std::sync::Arc;

use crate::app::{AppContext, LogItem};

pub async fn post_items(app: &AppContext, log_events: Vec<LogItem>) {
    let log_events = filter_events(app, log_events).await;

    {
        let mut hourly_statistics = app.hourly_statistics.lock().await;
        let mut telegram_notification_data = app.telegram_notification_data.lock().await;
        for itm in log_events.iter() {
            hourly_statistics.update(&itm);
            telegram_notification_data.update(&itm);
        }
    }

    if log_events.len() == 0 {
        return;
    }

    for log_event in log_events.iter() {
        app.insights_repo.try_insert_values(&log_event.ctx).await;
    }

    if let Some(elastic) = &app.elastic {
        elastic.logs_queue.add(log_events.clone()).await;
    }

    app.logs_queue.add(log_events).await;
}

async fn filter_events(app: &AppContext, mut log_events: Vec<LogItem>) -> Vec<Arc<LogItem>> {
    loop {
        let result = app
            .filter_events_cache
            .filter_events(log_events, |event, filter_events| {
                for filter in filter_events {
                    if filter.matches_ignore_filter(event) {
                        return false;
                    }
                }

                true
            })
            .await;

        match result {
            crate::cache::FilterEventResult::Ok(items) => {
                return items.into_iter().map(Arc::new).collect()
            }
            crate::cache::FilterEventResult::NotInitialized(items) => {
                log_events = items;
                let filter_events = app.settings_repo.get_ignore_events().await;
                app.filter_events_cache.apply(filter_events).await;
            }
        }
    }
}
