use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::AppContext, repo::dto::LogItemDto};

pub async fn search_and_scan(
    app: &AppContext,
    from_date: DateTimeAsMicroseconds,
    to_date: DateTimeAsMicroseconds,
    phrase: &str,
    limit: usize,
) -> Vec<LogItemDto> {
    let response = app
        .logs_repo
        .scan(from_date, to_date, phrase, limit, app.is_debug)
        .await;

    response
}
