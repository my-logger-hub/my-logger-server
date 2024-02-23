use crate::{app::LogItem, settings::TelegramSettings};

pub fn log_item_level_to_telegram_str(log_item: &LogItem) -> &str {
    match &log_item.level {
        my_logger::LogLevel::Info => "☑",
        my_logger::LogLevel::Warning => "⚠️Warning",
        my_logger::LogLevel::Error => "🟥Error",
        my_logger::LogLevel::FatalError => "☠️FatalError",
        my_logger::LogLevel::Debug => "🪲Debug",
    }
}

// Define a function to send a message using the Telegram Bot API
pub async fn send_message(telegram_settings: &TelegramSettings, log_item: &LogItem) {
    // Set the API endpoint and parameters
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        telegram_settings.api_key
    );

    let process = if let Some(process) = log_item.process.as_ref() {
        process
    } else {
        ""
    };

    let params = [
        ("chat_id", telegram_settings.chat_id.to_string()),
        (
            "message_thread_id",
            telegram_settings.message_thread_id.to_string(),
        ),
        ("parse_mode", "Markdown".to_string()),
        (
            "text",
            format!(
                "{}\n*EnvInfo*:{}\n*Process*: {}\n*Msg*: {}\n```{:#?}```",
                log_item_level_to_telegram_str(&log_item),
                telegram_settings.env_info,
                process,
                log_item.message,
                log_item.ctx
            ),
        ),
    ];

    // Create a client and send a POST request to the API

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let response = client.post(&url).form(&params).send().await;

    println!("{:?}", response);

    // Parse the JSON response
    //let telegram_response: TelegramResponse = response.json().await?;

    // Return the telegram response
}
