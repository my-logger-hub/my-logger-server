use std::collections::BTreeMap;

use my_http_server::{macros::MyHttpInput, types::RawData};
use my_logger::LogLevel;
use rust_extensions::{
    array_of_bytes_iterator::SliceIterator, date_time::DateTimeAsMicroseconds, lazy::LazyVec,
};

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

        let mut json_first_line_reader: my_json::json_reader::JsonFirstLineReader<SliceIterator> =
            bytes.into();

        while let Some(line) = json_first_line_reader.get_next() {
            let line = line.map_err(|_| format!("Failed to parse json at all"))?;

            let name = line
                .name
                .as_unescaped_name(&json_first_line_reader)
                .map_err(|_| "Can not read json name from param".to_string())?;

            if line.value.is_null(&json_first_line_reader) {
                continue;
            }

            match name {
                "@l" => match line
                    .value
                    .as_unescaped_str(&json_first_line_reader)
                    .unwrap()
                {
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
                    let value = line
                        .value
                        .as_unescaped_str(&json_first_line_reader)
                        .unwrap();
                    timestamp = DateTimeAsMicroseconds::from_str(value);
                }
                "Process" => {
                    let value = line
                        .value
                        .as_str(&json_first_line_reader)
                        .unwrap()
                        .to_string();
                    process = Some(value);
                }
                "@m" => {
                    let value = line
                        .value
                        .as_str(&json_first_line_reader)
                        .unwrap()
                        .to_string();
                    message = Some(value);
                }
                _ => {
                    let value = line
                        .value
                        .as_str(&json_first_line_reader)
                        .unwrap()
                        .to_string();
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
    use crate::app::LogItem;

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
}
