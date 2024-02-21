use crate::app::{AppContext, LogItem};

pub async fn post_items(app: &AppContext, log_events: Vec<LogItem>) {
    let log_events = app
        .settings_reader
        .filter_events(log_events, |event, filter_events| {
            for filter in filter_events {
                if filter.matches_ignore_filter(event) {
                    return false;
                }
            }

            true
        })
        .await;

    if log_events.len() == 0 {
        return;
    }

    app.logs_queue.add(log_events).await;
}
