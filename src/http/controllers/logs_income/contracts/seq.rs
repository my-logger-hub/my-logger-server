use std::collections::BTreeMap;

use my_http_server::{macros::MyHttpInput, types::RawData};
use my_json::json_reader::JsonFirstLineIterator;
use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::log_item::LogEvent;

#[derive(MyHttpInput)]
pub struct SeqInputHttpData {
    #[http_body_raw(description = "The Seq of the request")]
    pub body: RawData,
}

impl SeqInputHttpData {
    pub fn parse_log_events(&self) -> Vec<LogEvent> {
        let mut result = Vec::new();

        for chunk in self.body.as_slice().split(|itm| *itm == 13u8) {
            match LogEvent::parse_as_seq_payload(chunk) {
                Ok(log_data) => {
                    result.push(log_data);
                }
                Err(err) => {
                    println!(
                        "Failed to parse log data: {}. Err:{}",
                        std::str::from_utf8(chunk).unwrap(),
                        err
                    );
                }
            }
        }

        result
    }
}

impl LogEvent {
    pub fn parse_as_seq_payload(bytes: &[u8]) -> Result<Self, String> {
        let mut ctx = BTreeMap::new();

        let mut timestamp = None;

        let mut level = LogLevel::FatalError;
        let mut process = None;

        let mut message = None;

        let json_first_line_reader = JsonFirstLineIterator::new(bytes);

        while let Some(line) = json_first_line_reader.get_next() {
            let (name, value) = line.map_err(|_| format!("Failed to parse json at all"))?;

            let name = name
                .as_unescaped_str()
                .map_err(|_| "Can not read json name from param".to_string())?;

            if value.is_null() {
                continue;
            }

            match name {
                "@l" => match value.as_unescaped_str().unwrap() {
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
                    let value = value.as_unescaped_str().unwrap();
                    timestamp = DateTimeAsMicroseconds::from_str(value);
                }
                "Process" => {
                    let value = value.as_str().unwrap().to_string();
                    process = Some(value);
                }
                "@m" => {
                    let value = value.as_str().unwrap().to_string();
                    message = Some(value);
                }
                "@mt" => {
                    let value = value.as_str().unwrap().to_string();
                    message = Some(value);
                }

                _ => {
                    let value = value.as_str().unwrap().to_string();
                    ctx.insert(name.to_string(), value);
                }
            }
        }

        if message.is_none() {
            return Err("Can not find message in log".to_string());
        }
        Ok(Self {
            id: crate::utils::generate_log_id(),
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
    use crate::log_item::LogEvent;

    #[test]
    fn test() {
        let src = r#"{"@l":"Info","@t":"2023-08-11T21:02:45.660712+00:00","Process":"Table Schema verification","@m":"Db Schema is up to date for a table, trx_wallets","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687746+00:00","Process":"Table Schema verification","@m":"No Schema indexes is found for the table key_value. Indexes synchronization is skipping","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687785+00:00","Process":"Table Schema verification","@m":"Db Schema is up to date for a table, key_value","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.688846+00:00","Process":"TelemetryWriterTimer","@m":"Timer TelemetryWriterTimer is started with delay 1 sec","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687863+00:00","Process":"Starting Http Server","@m":"Http server starts at: 0.0.0.0:8000","Application":"trx-wallet-grpc","Version":"0.1.0"}"#;

        let item = LogEvent::parse_as_seq_payload(src.as_bytes()).unwrap();

        println!("item: {:?}", item);
    }
}
