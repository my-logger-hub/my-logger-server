use std::collections::BTreeMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{
    app::AppContext,
    repo::dto::{LogItemDto, LogLevelDto},
};

pub async fn search_logs(
    app: &AppContext,
    from_date: DateTimeAsMicroseconds,
    to_date: DateTimeAsMicroseconds,
    levels: Option<Vec<LogLevelDto>>,
    context: Option<BTreeMap<String, String>>,
    phrase: Option<&str>,
    limit: usize,
) -> Vec<LogItemDto> {
    let want_tantivy = has_tantivy_level(&levels);
    let want_sqlite = has_sqlite_level(&levels);

    let tantivy_fut = async {
        if want_tantivy {
            app.logs_repo
                .search(
                    from_date,
                    to_date,
                    levels.clone(),
                    context.clone(),
                    phrase,
                    limit,
                )
                .await
        } else {
            Vec::new()
        }
    };

    let sqlite_fut = async {
        if want_sqlite {
            app.sqlite_logs_repo
                .search(from_date, to_date, levels.clone(), context.clone(), phrase, limit)
                .await
        } else {
            Vec::new()
        }
    };

    let (mut tantivy_results, sqlite_results) = tokio::join!(tantivy_fut, sqlite_fut);

    tantivy_results.extend(sqlite_results);
    tantivy_results
        .sort_by(|a, b| b.moment.unix_microseconds.cmp(&a.moment.unix_microseconds));
    tantivy_results.truncate(limit);
    tantivy_results
}

fn has_tantivy_level(levels: &Option<Vec<LogLevelDto>>) -> bool {
    match levels {
        None => true,
        Some(list) => list.iter().any(|l| {
            matches!(
                l,
                LogLevelDto::Warning | LogLevelDto::Error | LogLevelDto::FatalError
            )
        }),
    }
}

fn has_sqlite_level(levels: &Option<Vec<LogLevelDto>>) -> bool {
    match levels {
        None => true,
        Some(list) => list
            .iter()
            .any(|l| matches!(l, LogLevelDto::Debug | LogLevelDto::Info)),
    }
}
