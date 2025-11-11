use std::collections::BTreeMap;

use my_http_server::{macros::MyHttpInput, types::RawData};
use my_json::json_reader::JsonFirstLineIterator;
use my_logger::LogLevel;
use rust_extensions::{date_time::DateTimeAsMicroseconds, lazy::LazyVec, SortableId};

use crate::app::LogItem;

#[derive(MyHttpInput)]
pub struct SeqInputHttpData {
    #[http_body_raw(description = "The Seq of the request")]
    pub body: RawData,
}

impl SeqInputHttpData {
    pub fn parse_log_events(&self) -> Option<Vec<LogItem>> {
        let mut result = LazyVec::new();

        for chunk in self.body.as_slice().split(|itm| *itm == 13u8) {
            match LogItem::parse_as_seq_payload(chunk) {
                Ok(log_data) => {
                    result.add(log_data);
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

        result.get_result()
    }
}

impl LogItem {
    pub fn parse_as_seq_payload(bytes: &[u8]) -> Result<Self, String> {
        let mut ctx = BTreeMap::new();

        let mut timestamp = None;

        let mut level = LogLevel::FatalError;
        let mut process = None;

        let mut message = None;

        let json_first_line_reader = JsonFirstLineIterator::new(bytes);

        while let Some(line) = json_first_line_reader.get_next() {
            let (name, value) =
                line.map_err(|err| format!("Failed to parse json at all. {:?}", err))?;

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
                "MessageTemplate" => {
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
            id: SortableId::generate().into(),
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
    use my_http_server::types::RawData;

    use crate::{app::LogItem, http::controllers::logs_income::contracts::SeqInputHttpData};

    #[test]
    fn test() {
        let src = r#"{"@l":"Info","@t":"2023-08-11T21:02:45.660712+00:00","Process":"Table Schema verification","@m":"Db Schema is up to date for a table, trx_wallets","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687746+00:00","Process":"Table Schema verification","@m":"No Schema indexes is found for the table key_value. Indexes synchronization is skipping","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687785+00:00","Process":"Table Schema verification","@m":"Db Schema is up to date for a table, key_value","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.688846+00:00","Process":"TelemetryWriterTimer","@m":"Timer TelemetryWriterTimer is started with delay 1 sec","Application":"trx-wallet-grpc","Version":"0.1.0"}
{"@l":"Info","@t":"2023-08-11T21:02:45.687863+00:00","Process":"Starting Http Server","@m":"Http server starts at: 0.0.0.0:8000","Application":"trx-wallet-grpc","Version":"0.1.0"}"#;

        let item = LogItem::parse_as_seq_payload(src.as_bytes()).unwrap();

        println!("item: {:?}", item);
    }

    #[test]
    fn test2() {
        let src: Vec<u8> = vec![
            123, 34, 64, 108, 34, 58, 34, 70, 97, 116, 97, 108, 34, 44, 34, 64, 116, 34, 58, 34,
            50, 48, 50, 53, 45, 48, 56, 45, 49, 52, 84, 50, 48, 58, 48, 52, 58, 52, 51, 46, 51, 52,
            51, 49, 57, 52, 34, 44, 34, 80, 114, 111, 99, 101, 115, 115, 34, 58, 34, 80, 97, 110,
            105, 99, 32, 72, 97, 110, 100, 108, 101, 114, 34, 44, 34, 64, 109, 34, 58, 34, 92, 34,
            115, 116, 97, 116, 117, 115, 58, 32, 85, 110, 97, 118, 97, 105, 108, 97, 98, 108, 101,
            44, 32, 109, 101, 115, 115, 97, 103, 101, 58, 32, 92, 92, 92, 34, 67, 111, 110, 110,
            101, 99, 116, 105, 111, 110, 32, 114, 101, 102, 117, 115, 101, 100, 32, 40, 111, 115,
            32, 101, 114, 114, 111, 114, 32, 49, 49, 49, 41, 92, 92, 92, 34, 44, 32, 100, 101, 116,
            97, 105, 108, 115, 58, 32, 91, 93, 44, 32, 109, 101, 116, 97, 100, 97, 116, 97, 58, 32,
            77, 101, 116, 97, 100, 97, 116, 97, 77, 97, 112, 32, 123, 32, 104, 101, 97, 100, 101,
            114, 115, 58, 32, 123, 125, 32, 125, 92, 34, 34, 44, 34, 65, 112, 112, 108, 105, 99,
            97, 116, 105, 111, 110, 34, 58, 34, 97, 105, 45, 98, 114, 105, 100, 103, 101, 45, 103,
            114, 112, 99, 34, 44, 34, 86, 101, 114, 115, 105, 111, 110, 34, 58, 34, 48, 46, 49, 46,
            49, 55, 34, 44, 34, 69, 110, 118, 73, 110, 102, 111, 34, 58, 34, 86, 77, 45, 48, 49,
            34, 44, 34, 76, 111, 99, 97, 116, 105, 111, 110, 34, 58, 34, 115, 114, 99, 47, 103,
            114, 112, 99, 95, 99, 108, 105, 101, 110, 116, 47, 97, 105, 95, 99, 104, 97, 116, 95,
            104, 105, 115, 116, 111, 114, 121, 95, 103, 114, 112, 99, 95, 99, 108, 105, 101, 110,
            116, 46, 114, 115, 58, 51, 58, 49, 34, 44, 34, 80, 97, 110, 105, 99, 73, 110, 102, 111,
            34, 58, 34, 112, 97, 110, 105, 99, 107, 101, 100, 32, 97, 116, 32, 115, 114, 99, 47,
            103, 114, 112, 99, 95, 99, 108, 105, 101, 110, 116, 47, 97, 105, 95, 99, 104, 97, 116,
            95, 104, 105, 115, 116, 111, 114, 121, 95, 103, 114, 112, 99, 95, 99, 108, 105, 101,
            110, 116, 46, 114, 115, 58, 51, 58, 49, 58, 115, 116, 97, 116, 117, 115, 58, 32, 85,
            110, 97, 118, 97, 105, 108, 97, 98, 108, 101, 44, 32, 109, 101, 115, 115, 97, 103, 101,
            58, 32, 92, 34, 67, 111, 110, 110, 101, 99, 116, 105, 111, 110, 32, 114, 101, 102, 117,
            115, 101, 100, 32, 40, 111, 115, 32, 101, 114, 114, 111, 114, 32, 49, 49, 49, 41, 92,
            34, 44, 32, 100, 101, 116, 97, 105, 108, 115, 58, 32, 91, 93, 44, 32, 109, 101, 116,
            97, 100, 97, 116, 97, 58, 32, 77, 101, 116, 97, 100, 97, 116, 97, 77, 97, 112, 32, 123,
            32, 104, 101, 97, 100, 101, 114, 115, 58, 32, 123, 125, 32, 125, 34, 125,
        ];

        let data = SeqInputHttpData {
            body: RawData::new(src),
        };

        let payload = data.parse_log_events().unwrap();

        println!("payload: {:?}", payload);
    }
}
