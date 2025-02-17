use crate::{app::AppContext, repo::ignore_single_events::*};

pub async fn add(app: &AppContext, item: IgnoreSingleEventModel) {
    app.ignore_single_events_repo.add(item).await;

    let telegram_settings = app.settings_reader.get_telegram_settings().await;

    if let Some(telegram_settings) = telegram_settings {
        crate::telegram::api::send_text_message(&telegram_settings, "Ignore single event added")
            .await;
    }
}
