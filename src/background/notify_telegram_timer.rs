use std::sync::Arc;

use rust_extensions::{date_time::DateTimeAsMicroseconds, MyTimerTick};

use crate::app::AppContext;

pub struct NotifyTelegramTimer {
    pub app: Arc<AppContext>,
}

impl NotifyTelegramTimer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl MyTimerTick for NotifyTelegramTimer {
    async fn tick(&self) {
        let now = DateTimeAsMicroseconds::now();
        let to_telegram = self
            .app
            .telegram_notification_data
            .get_something_to_notify(now)
            .await;

        if to_telegram.is_none() {
            return;
        }

        let to_telegram = to_telegram.unwrap();

        let telegram_settings = self.app.settings_reader.get_telegram_settings().await;

        if telegram_settings.is_none() {
            println!("Telegram is disabled");
            return;
        }

        let telegram_settings = telegram_settings.unwrap();

        let ui_url = self.app.get_ui_url().await;

        crate::telegram::api::send_notification_data(
            &telegram_settings,
            &to_telegram,
            &self.app.env_name,
            ui_url,
        )
        .await;
    }
}
