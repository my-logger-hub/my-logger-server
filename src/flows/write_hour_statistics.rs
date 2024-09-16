use std::collections::BTreeMap;

use crate::{app::AppContext, repo::HourStatisticsDto};

pub async fn write_hour_statistics(app: &AppContext) {
    let to_persist = {
        let mut hourly_statistics = app.hourly_statistics.lock().await;

        println!(
            "Persisting statistics for {:?} hours",
            hourly_statistics.to_persist
        );
        if hourly_statistics.to_persist.len() == 0 {
            return;
        }

        let mut to_persist = BTreeMap::new();
        std::mem::swap(&mut hourly_statistics.to_persist, &mut to_persist);
        to_persist
    };

    for (hour, by_date) in to_persist {
        for (app_name, item) in by_date {
            let dto = HourStatisticsDto {
                date_key: hour.get_value(),
                app: app_name.clone(),
                info: item.info,
                warning: item.warning,
                error: item.error,
                fatal_error: item.fatal_error,
                debug: item.debug,
            };
            app.hour_statistics_repo.insert_or_update(&dto).await;
        }
    }
}
