use crate::app::AppContext;

pub async fn delete(app: &AppContext, id: &str) {
    app.ignore_single_events_repo.delete(id).await;

    let telegram_settings = app.settings_reader.get_telegram_settings().await;

    if let Some(telegram_settings) = telegram_settings {
        crate::telegram::api::send_text_message(&telegram_settings, "Ignore single event removed")
            .await;
    }
}
