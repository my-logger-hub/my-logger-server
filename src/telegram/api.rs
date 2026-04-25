use std::fmt::Write;

use flurl::{body::UrlEncodedBody, FlUrl};
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{app::LogItem, settings::TelegramSettings};

use super::NotificationItem;

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
    ui_url: String,
) {

    if notification_data.fatal_errors == 0 && notification_data.errors == 0 {
        return;
    }
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        telegram_settings.api_key
    );

    let time_interval: DateTimeAsMicroseconds = notification_data.key.try_into().unwrap();

    let mut telegram_statistics = String::with_capacity(256);
    let _ = write!(
        telegram_statistics,
        "---\n📊<b>EnvInfo</b>:{}\n<b>Statistics of minute</b>: {}\n☠️<b>FatalErrors</b>: {}\n🟥<b>Errors</b>: {}\n⚠️<b>Warnings</b>: {}\n",
        env_name,
        time_interval.to_rfc3339(),
        notification_data.fatal_errors,
        notification_data.errors,
        notification_data.warnings,
    );
    if !ui_url.is_empty() {
        let _ = write!(telegram_statistics, "<a href=\"{}\">LogsUi</a>", ui_url);
    }
    telegram_statistics.push('\n');

    println!("Sending telegram stats: {}", telegram_statistics);

    let mut chat_id_buf = itoa::Buffer::new();
    let mut thread_id_buf = itoa::Buffer::new();

    let body = UrlEncodedBody::new()
        .append("chat_id", chat_id_buf.format(telegram_settings.chat_id))
        .append(
            "message_thread_id",
            thread_id_buf.format(telegram_settings.message_thread_id),
        )
        .append("parse_mode", "HTML")
        .append("text", &telegram_statistics);

    let response = FlUrl::new(url.as_str())
        .accept_invalid_certificate()
        .with_retries(3)
        .post(body)
        .await;

    log_telegram_response("send_notification_data", response).await;

    println!("Minute Statistics{:?}", notification_data);
}
// Define a function to send a message using the Telegram Bot API
pub async fn send_log_item(
    telegram_settings: &TelegramSettings,
    log_item: &LogItem,
    env_name: &str,
) {
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        telegram_settings.api_key
    );

    let process = log_item.process.as_deref().unwrap_or("");

    let mut text = String::with_capacity(512 + log_item.message.len() + log_item.ctx.len() * 32);
    let _ = write!(
        text,
        "---\n{}\n{}\n<b>EnvInfo</b>:",
        log_item.timestamp.to_rfc3339(),
        log_item_level_to_telegram_str(log_item),
    );
    append_telegram_escaped(&mut text, env_name);
    text.push_str("\n<b>Process</b>: ");
    append_telegram_escaped(&mut text, process);
    text.push_str("\n<b>Msg</b>: ");
    append_telegram_escaped(&mut text, &log_item.message);
    text.push_str("\n```Context:\n");
    append_code_escaped_debug(&mut text, &log_item.ctx);
    text.push_str("\n```\n");

    let mut chat_id_buf = itoa::Buffer::new();
    let mut thread_id_buf = itoa::Buffer::new();

    let body = UrlEncodedBody::new()
        .append("chat_id", chat_id_buf.format(telegram_settings.chat_id))
        .append(
            "message_thread_id",
            thread_id_buf.format(telegram_settings.message_thread_id),
        )
        .append("parse_mode", "Markdown")
        .append("text", &text);

    let response = FlUrl::new(url.as_str())
        .accept_invalid_certificate()
        .with_retries(3)
        .post(body)
        .await;

    log_telegram_response("send_log_item", response).await;
}

async fn log_telegram_response(
    fn_name: &str,
    response: Result<flurl::FlUrlResponse, flurl::FlUrlError>,
) {
    match response {
        Ok(mut response) => {
            let status = response.get_status_code();
            if !(200..300).contains(&status) {
                let body = response.get_body_as_slice().await;
                let body_str = body
                    .as_ref()
                    .ok()
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .unwrap_or("<binary>");
                println!(
                    "{}: telegram non-2xx status {}, body: {}",
                    fn_name, status, body_str
                );
            }
        }
        Err(err) => {
            println!("{}: telegram request failed: {:?}", fn_name, err);
        }
    }
}

fn append_telegram_escaped(dst: &mut String, src: &str) {
    dst.reserve(src.len());
    for c in src.chars() {
        if c <= ' ' {
            dst.push(' ');
            continue;
        }
        match c {
            '_' | '*' | '[' | ']' | '(' | ')' | '~' | '`' | '>' | '#' | '+' | '-' | '=' | '|'
            | '{' | '}' | '.' | '!' => {
                dst.push('\\');
                dst.push(c);
            }
            _ => dst.push(c),
        }
    }
}

fn append_code_escaped_debug(dst: &mut String, ctx: &std::collections::BTreeMap<String, String>) {
    use std::fmt::Write;
    struct Escaper<'a>(&'a mut String);
    impl<'a> std::fmt::Write for Escaper<'a> {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0.reserve(s.len());
            for c in s.chars() {
                match c {
                    '`' | '_' => {
                        self.0.push('\\');
                        self.0.push(c);
                    }
                    _ => self.0.push(c),
                }
            }
            Ok(())
        }
    }
    let _ = write!(Escaper(dst), "{:#?}", ctx);
}
