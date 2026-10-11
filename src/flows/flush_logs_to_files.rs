use crate::app::AppContext;

/// Writes the records which have waited in memory long enough - or all of them
pub async fn flush_logs_to_files(app: &AppContext, all: bool) {
    let ready = app.logs_storage.take_ready(all);

    if ready.is_empty() && !all {
        return;
    }

    let storage = app.logs_storage.clone();

    let messages = tokio::task::spawn_blocking(move || {
        let mut messages = storage.write(ready);

        // Before the process ends the key indexes are written whatever their time
        if all {
            messages.extend(storage.persist_key_indexes(true));
        }

        messages
    })
    .await
    .unwrap();

    for message in messages {
        println!("{}", message);
        app.debug_console.push(message).await;
    }
}
