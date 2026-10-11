use std::collections::BTreeMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{
    app::AppContext,
    logs_storage::{SearchDirection, SearchFilter},
    repo::dto::{LogItemDto, LogLevelDto},
};

/// The records of the product scope in the range which match, at most `limit` of them:
/// from `from_date` up, the oldest first, or from `to_date` down, the latest first
pub async fn search_logs(
    app: &AppContext,
    product_scope: &str,
    from_date: DateTimeAsMicroseconds,
    to_date: DateTimeAsMicroseconds,
    levels: Option<Vec<LogLevelDto>>,
    context: Option<BTreeMap<String, String>>,
    phrase: Option<&str>,
    limit: usize,
    direction: SearchDirection,
) -> Vec<LogItemDto> {
    let filter = SearchFilter::new(levels, context, phrase);
    let storage = app.logs_storage.clone();
    let product_scope = product_scope.to_string();
    let runtime = tokio::runtime::Handle::current();

    // The blocks are decompressed as they are read: off the workers of the runtime
    tokio::task::spawn_blocking(move || {
        runtime.block_on(storage.search(
            &product_scope,
            from_date.unix_microseconds,
            to_date.unix_microseconds,
            &filter,
            limit,
            direction,
        ))
    })
    .await
    .unwrap()
}
