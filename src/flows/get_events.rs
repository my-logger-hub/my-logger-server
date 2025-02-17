use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::AppContext, repo::logs::*};

pub async fn get_events(
    app: &AppContext,
    levels: Vec<LogLevel>,
    context_keys: Vec<LogEventCtxFileGrpcModel>,
    from_date: DateTimeAsMicroseconds,
    to_date: DateTimeAsMicroseconds,
    take: usize,
) -> Vec<LogEventFileGrpcModel> {
    let response = app
        .logs_repo
        .scan(from_date, to_date, take, &|itm| {
            let result = itm.filter_by_log_level(levels.as_slice())
                && itm.filter_by_ctx(context_keys.as_slice());
            Some(result)
        })
        .await;

    response
}
