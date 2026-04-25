use crate::{app::AppContext, hourly_statistics::PersistedHour};

pub async fn init(app: &AppContext) {
    let bytes = match tokio::fs::read(&app.statistics_path).await {
        Ok(b) => b,
        Err(_) => {
            println!(
                "No statistics file at {}, starting empty",
                app.statistics_path
            );
            return;
        }
    };

    let parsed: Vec<PersistedHour> = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "Failed to parse statistics file at {}: {}",
                app.statistics_path, e
            );
            return;
        }
    };

    let count = parsed.len();
    let mut access = app.hourly_statistics.lock().await;
    access.restore_from_vec(parsed);
    println!(
        "Restored statistics for {} hours from {}",
        count, app.statistics_path
    );
}
