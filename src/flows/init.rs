use crate::{app::AppContext, hourly_statistics::MAX_HOURS_TO_KEEP};

pub async fn init(app: &AppContext) {
    let keys = app
        .hour_statistics_repo
        .get_top_keys(MAX_HOURS_TO_KEEP)
        .await;

    println!("Restoring statistics for {:?} hours", keys);

    if keys.len() == 0 {
        return;
    }

    let result = app.hour_statistics_repo.get_by_keys(keys).await;

    let mut write_access = app.hourly_statistics.lock().await;

    for itm in result {
        write_access.restore(itm);
    }
}
