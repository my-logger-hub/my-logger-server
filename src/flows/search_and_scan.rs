use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::AppContext, logs_storage::SearchDirection, repo::dto::LogItemDto};

pub async fn search_and_scan(
    app: &AppContext,
    product_scope: &str,
    from_date: DateTimeAsMicroseconds,
    to_date: DateTimeAsMicroseconds,
    phrase: &str,
    limit: usize,
) -> Vec<LogItemDto> {
    super::search_logs(
        app,
        product_scope,
        from_date,
        to_date,
        None,
        None,
        Some(phrase),
        limit,
        SearchDirection::Down,
    )
    .await
}
