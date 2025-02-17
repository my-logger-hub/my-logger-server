use std::io::SeekFrom;

use my_logger::LogLevel;
use rust_extensions::file_utils::FilePath;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::my_logger_grpc::*;

use super::TenMinLogIterator;

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LogEventCtxFileGrpcModel {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

impl Into<LogEventCtxFileGrpcModel> for LogEventContext {
    fn into(self) -> LogEventCtxFileGrpcModel {
        LogEventCtxFileGrpcModel {
            key: self.key,
            value: self.value,
        }
    }
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LogEventFileGrpcModel {
    #[prost(sint64, tag = "1")]
    pub timestamp: i64,

    #[prost(string, tag = "2")]
    pub id: String,

    #[prost(int32, tag = "3")]
    pub level: i32,

    #[prost(string, tag = "4")]
    pub message: String,

    #[prost(string, optional, tag = "5")]
    pub process: Option<String>,

    #[prost(repeated, message, tag = "6")]
    pub ctx: Vec<LogEventCtxFileGrpcModel>,
}

impl LogEventFileGrpcModel {
    pub fn remove_key(&mut self, name: &str) -> Option<LogEventCtxFileGrpcModel> {
        let index = self.ctx.iter().position(|itm| itm.key == name)?;

        let result = self.ctx.remove(index);
        Some(result)
    }

    pub fn get_log_level(&self) -> LogLevel {
        LogLevel::from_u8(self.level as u8)
    }

    pub fn filter_by_log_level(&self, levels: &[LogLevel]) -> bool {
        if levels.len() == 0 {
            return true;
        }

        for to_match in levels {
            if to_match.to_u8() == self.level as u8 {
                return true;
            }
        }

        false
    }

    fn has_ctx(&self, key: &str, value: &str) -> bool {
        for ctx in self.ctx.iter() {
            if ctx.key == key && ctx.value == value {
                return true;
            }
        }

        false
    }

    pub fn filter_by_ctx<'k, 'v>(&self, ctx_in: &[LogEventCtxFileGrpcModel]) -> bool {
        if ctx_in.len() == 0 {
            return true;
        }

        for ctx_in in ctx_in {
            if !self.has_ctx(&ctx_in.key, &ctx_in.value) {
                return false;
            }
        }

        true
    }

    pub fn filter_by_phrase(&self, phrase: &str) -> bool {
        if self.message.contains(phrase) {
            return true;
        }

        if let Some(process) = self.process.as_ref() {
            if process.contains(phrase) {
                return true;
            }
        }

        for itm in self.ctx.iter() {
            if itm.value.contains(phrase) {
                return true;
            }
        }

        false
    }
}

pub struct TenMinLog {
    pub file: tokio::fs::File,
    file_pos: u64,
}

impl TenMinLog {
    pub async fn open(file_path: &FilePath) -> Option<Self> {
        let file = tokio::fs::File::open(file_path.as_str()).await;

        if let Err(err) = &file {
            /*
            println!(
                "Can not open file {}. Reason: {:?}. Considering file as not exists",
                file_path.as_str(),
                err,
            );
             */
            return None;
        }

        let file = file.unwrap();

        let meta_data = file.metadata().await.unwrap();

        let result = Self {
            file,
            file_pos: meta_data.len(),
        };

        Some(result)
    }

    pub async fn open_or_create(file_path: &FilePath) -> Self {
        let file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true) // Create if it doesn't exist
            .open(file_path.as_str())
            .await;

        if let Err(err) = &file {
            panic!(
                "Can not open or create file {}. Err: {}",
                file_path.as_str(),
                err
            );
        }

        let file = file.unwrap();
        let meta_data = file.metadata().await.unwrap();

        let result = Self {
            file,
            file_pos: meta_data.len(),
        };

        result
    }

    pub async fn upload_logs(&mut self, events: &[LogEventFileGrpcModel]) {
        let mut to_upload_content = Vec::new();

        for event in events {
            let mut message = Vec::new();
            prost::Message::encode(event, &mut message).unwrap();

            let len = message.len() as u32;

            to_upload_content.extend_from_slice(len.to_be_bytes().as_slice());
            to_upload_content.extend_from_slice(&message);
        }

        let file_pos = self.file_pos;
        self.file.seek(SeekFrom::Start(file_pos)).await.unwrap();
        self.file.write_all(&to_upload_content).await.unwrap();
        self.file_pos += to_upload_content.len() as u64
    }

    pub async fn iter<'s>(&'s mut self) -> TenMinLogIterator<'s> {
        self.file.seek(SeekFrom::Start(0)).await.unwrap();
        TenMinLogIterator::new(self)
    }
}

#[cfg(test)]
mod tests {
    use rust_extensions::date_time::DateTimeAsMicroseconds;

    use super::{LogEventCtxFileGrpcModel, LogEventFileGrpcModel, TenMinLog};

    #[tokio::test]
    async fn tests() {
        let items = vec![LogEventFileGrpcModel {
            timestamp: DateTimeAsMicroseconds::now().unix_microseconds,
            id: "1".to_string(),
            level: 0,
            process: None,
            message: "Test Message".to_string(),
            ctx: vec![LogEventCtxFileGrpcModel {
                key: "id".to_string(),
                value: "value".to_string(),
            }],
        }];

        let file_path: rust_extensions::file_utils::FilePath = "~/test.log".into();

        let _ = tokio::fs::remove_file(file_path.as_str()).await;

        let mut file = TenMinLog::open(&file_path).await.unwrap();

        file.upload_logs(&items).await;

        let logs_iterator = file.iter().await;

        let mut result = Vec::new();

        while let Some(item) = logs_iterator.get_next().await {
            result.push(item);
        }

        println!("{:?}", result);
    }
}
