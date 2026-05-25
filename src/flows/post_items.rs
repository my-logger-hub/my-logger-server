use std::sync::Arc;

use crate::app::{AppContext, LogItem};

pub async fn post_items(app: &AppContext, log_events: Vec<LogItem>) {
    let log_events = filter_events(app, log_events).await;

    let log_events = filter_single_ignore_events(app, log_events).await;

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

    let (low, high): (Vec<Arc<LogItem>>, Vec<Arc<LogItem>>) =
        log_events.into_iter().partition(|e| {
            matches!(
                e.level,
                my_logger::LogLevel::Debug | my_logger::LogLevel::Info
            )
        });

    if !low.is_empty() {
        app.sqlite_logs_queue.add(low).await;
    }
    if !high.is_empty() {
        app.logs_queue.add(high).await;
    }
}

async fn filter_single_ignore_events(
    app: &AppContext,
    log_events: Vec<Arc<LogItem>>,
) -> Vec<Arc<LogItem>> {
    let mut cache = app.ignore_single_event_cache.lock().await;

    if !cache.initialized {
        let items = super::ignore_single_event::persistence::get_all(app).await;
        cache.init(items);
    }

    log_events
        .into_iter()
        .filter(|itm| !cache.skip_by_filtering(itm))
        .collect()
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
