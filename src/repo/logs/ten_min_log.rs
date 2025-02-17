use std::{io::SeekFrom, sync::atomic::AtomicI64};

use rust_extensions::file_utils::FilePath;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use super::*;

pub struct TenMinLog {
    pub file: tokio::fs::File,
    file_pos: u64,
}

impl TenMinLog {
    pub async fn open(file_path: &FilePath) -> Option<Self> {
        let file = tokio::fs::File::open(file_path.as_str()).await;

        if file.is_err() {
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
