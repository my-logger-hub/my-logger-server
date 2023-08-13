use my_http_server::types::RawData;
use my_http_server_swagger::MyHttpInput;
use my_logger::LogLevel;
use rust_extensions::{date_time::DateTimeAsMicroseconds, lazy::LazyVec};

use crate::app::{LogCtxItem, LogItem};

#[derive(MyHttpInput)]
pub struct SeqInputHttpData {
    #[http_body_raw(description = "The Seq of the request")]
    pub body: RawData,
}

impl SeqInputHttpData {
    pub fn parse_log_events(&self, tenant: &str) -> Option<Vec<LogItem>> {
        let mut result = LazyVec::new();

        for chunk in self.body.as_slice().split(|itm| *itm == 13u8) {
            match LogItem::parse_as_seq_payload(chunk, tenant) {
                Ok(log_data) => {
                    result.add(log_data);
                }
                Err(_) => {
                    println!(
                        "Failed to parse log data: {}",
                        std::str::from_utf8(chunk).unwrap()
                    );
                }
            }
        }

        result.get_result()
    }
}

impl LogItem {
    pub fn parse_as_seq_payload(bytes: &[u8], tenant: &str) -> Result<Self, String> {
        let mut ctx = Vec::new();

        let mut timestamp = None;

        let mut level = LogLevel::FatalError;
        let mut process = None;

        let mut message = None;

        for first_line in my_json::json_reader::JsonFirstLineReader::new(bytes) {
            let first_line = first_line.map_err(|_| format!("Failed to parse json at all"))?;

            let name = first_line
                .get_name()
                .map_err(|_| "Can not read json name from param".to_string())?;

            let value = first_line
                .get_value()
                .map_err(|_| format!("Can not read json value for param '{}'", name))?;

            let value = value.as_str();

            if value.is_none() {
                continue;
            }

            let value = value.unwrap();

            match name {
                "@l" => match value {
                    "Info" => {
                        level = LogLevel::Info;
                    }
                    "Warning" => {
                        level = LogLevel::Warning;
                    }
                    "Error" => {
                        level = LogLevel::Error;
                    }
                    "Debug" => {
                        level = LogLevel::Debug;
                    }
                    _ => {}
                },
                "@t" => {
                    timestamp = DateTimeAsMicroseconds::from_str(value);
                }
                "Process" => {
                    process = Some(value.to_string());
                }
                "@m" => {
                    message = Some(value.to_string());
                }
                _ => {
                    ctx.push(LogCtxItem {
                        key: name.to_string(),
                        value: value.to_string(),
                    });
                }
            }
        }

        if message.is_none() {
            return Err("Can not find message in log".to_string());
        }
        Ok(Self {
            id: crate::utils::generate_log_id(),
            tenant: tenant.to_string(),
            level,
            timestamp: if let Some(timestamp) = timestamp {
                timestamp
            } else {
                DateTimeAsMicroseconds::now()
            },
            process,
            message: message.unwrap(),
            ctx,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::app::LogItem;

    #[test]
    fn test() {
        let src = r#"{"@l":"Info","@t":"2023-08-11T21:02:45.660712+00:00","Process":"Table Schema verification","@m":"Db Schema is up to date for a table, trx_wallets","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687746+00:00","Process":"Table Schema verification","@m":"No Schema indexes is found for the table key_value. Indexes synchronization is skipping","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687785+00:00","Process":"Table Schema verification","@m":"Db Schema is up to date for a table, key_value","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.688846+00:00","Process":"TelemetryWriterTimer","@m":"Timer TelemetryWriterTimer is started with delay 1 sec","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687863+00:00","Process":"Starting Http Server","@m":"Http server starts at: 0.0.0.0:8000","Application":"trx-wallet-grpc","Version":"0.1.0"}"#;

        let item = LogItem::parse_as_seq_payload(src.as_bytes(), "test").unwrap();

        println!("item: {:?}", item);
    }
}
