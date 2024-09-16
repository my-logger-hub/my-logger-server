use std::collections::BTreeMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::AppContext, my_logger_grpc::*, repo::dto::LogItemDto};

pub async fn get_events(
    app: &AppContext,
    levels: Vec<LogLevelGrpcModel>,
    context_keys: Vec<LogEventContext>,
    from_date: DateTimeAsMicroseconds,
    to_date: Option<DateTimeAsMicroseconds>,
    take: usize,
) -> Vec<LogItemDto> {
    let log_levels = if levels.len() > 0 {
        Some(levels.into_iter().map(|level| level.into()).collect())
    } else {
        None
    };

    let context = if context_keys.len() > 0 {
        let mut ctx = BTreeMap::new();
        for itm in context_keys {
            ctx.insert(itm.key, itm.value);
        }
        Some(ctx)
    } else {
        None
    };

    let response = app
        .logs_repo
        .get(from_date, to_date, log_levels, context, take)
        .await;

    response
}
