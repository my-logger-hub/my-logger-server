use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::LogItem, settings::TelegramSettings};

use super::{NotificationItem};

pub fn log_item_level_to_telegram_str(log_item: &LogItem) -> &str {
    match &log_item.level {
        my_logger::LogLevel::Info => "☑",
        my_logger::LogLevel::Warning => "⚠️Warning",
        my_logger::LogLevel::Error => "🟥Error",
        my_logger::LogLevel::FatalError => "☠️FatalError",
        my_logger::LogLevel::Debug => "🪲Debug",
    }
}

pub async fn send_notification_data(
    telegram_settings: &TelegramSettings,
    notification_data: &NotificationItem,
    env_name: &str,
) {
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        telegram_settings.api_key
    );

    let time_interval: DateTimeAsMicroseconds = notification_data.key.clone().try_into().unwrap();
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
                "---\n📊*EnvInfo*:{}\n*Statistics of minute*: {}\n*FatalErrors*: {}\n*Errors*: {}\n*Warnings*: {}\n",
                env_name,
                time_interval.to_rfc3339(),
                notification_data.fatal_errors,
                notification_data.errors,
                notification_data.warnings,                
            ),
        ),
    ];

    // Create a client and send a POST request to the API

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let _ = client.post(&url).form(&params).send().await;

    println!("Minute Statistics{:?}", notification_data);
}
// Define a function to send a message using the Telegram Bot API
pub async fn send_log_item(
    telegram_settings: &TelegramSettings,
    log_item: &LogItem,
    env_name: &str,
) {
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
                "---\n{}\n{}\n*EnvInfo*:{}\n*Process*: {}\n*Msg*: {}\n```Context:\n{}\n```\n",
                log_item.timestamp.to_rfc3339(),
                log_item_level_to_telegram_str(&log_item),
                format_telegram_message(env_name),
                format_telegram_message(&process),
                format_telegram_message(&log_item.message),
                format_code_telegram_message(format!("{:#?}", log_item.ctx))
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

fn format_telegram_message(src: &str) -> String {
    let mut result = String::new();

    for c in src.chars() {
        if c <= ' ' {
            result.push(' ');
        } else {
            match c {
                '_' => {
                    result.push_str("\\_");
                }
                '*' => {
                    result.push_str("\\*");
                }
                '[' => {
                    result.push_str("\\[");
                }
                ']' => {
                    result.push_str("\\]");
                }
                '(' => {
                    result.push_str("\\(");
                }
                ')' => {
                    result.push_str("\\)");
                }
                '~' => {
                    result.push_str("\\~");
                }
                '`' => {
                    result.push_str("\\`");
                }
                '>' => {
                    result.push_str("\\>");
                }
                '#' => {
                    result.push_str("\\#");
                }
                '+' => {
                    result.push_str("\\+");
                }
                '-' => {
                    result.push_str("\\-");
                }
                '=' => {
                    result.push_str("\\=");
                }
                '|' => {
                    result.push_str("\\|");
                }
                '{' => {
                    result.push_str("\\{");
                }
                '}' => {
                    result.push_str("\\}");
                }
                '.' => {
                    result.push_str("\\.");
                }
                '!' => {
                    result.push_str("\\!");
                }
                _ => {
                    result.push(c);
                }
            }
        }
    }

    result
}

fn format_code_telegram_message(src: String) -> String {
    let mut result = String::new();

    for c in src.chars() {
        match c {
            '`' => {
                result.push_str("\\`");
            }
            '_' => {
                result.push_str("\\_");
            }
            _ => {
                result.push(c);
            }
        }
    }
    result
}
