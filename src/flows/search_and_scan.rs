use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::AppContext, repo::logs::LogEventFileGrpcModel};

pub async fn search_and_scan(
    app: &AppContext,
    from_date: DateTimeAsMicroseconds,
    to_date: DateTimeAsMicroseconds,
    phrase: &str,
    limit: usize,
) -> Vec<LogEventFileGrpcModel> {
    let response = app
        .logs_repo
        .scan(from_date, Some(to_date), limit, &|itm| {
            Some(itm.filter_by_phrase(phrase))
        })
        .await;

    response
}
