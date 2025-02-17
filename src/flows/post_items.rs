use std::sync::Arc;

use crate::{app::AppContext, log_item::LogEvent};

pub async fn post_items(app: &AppContext, mut log_events: Vec<LogEvent>) {
    if log_events.len() == 0 {
        return;
    }

    app.ignore_events_repo.filter(&mut log_events).await;

    app.hour_statistics_repo
        .new_events(log_events.as_slice())
        .await;

    let mut log_events: Vec<_> = log_events.into_iter().map(Arc::new).collect();

    let filtering_data = app
        .ignore_single_events_repo
        .find_matching_events(log_events.as_slice())
        .await;

    if filtering_data.len() > 0 {
        let mut ignore_data_access = app.ignore_single_event_cache.lock().await;

        for (log_event, ignore_single_event) in filtering_data {
            if ignore_data_access.skip_it(&ignore_single_event) {
                log_events.retain(|itm| itm.id != log_event.id);
            }
        }
    }

    if let Some(elastic) = &app.elastic {
        elastic.logs_queue.add(log_events.clone()).await;
    }

    app.logs_queue.add(log_events).await;
}
