use tokio::{io::AsyncReadExt, sync::Mutex};

use super::{LogEventFileGrpcModel, TenMinLog};

pub struct TenMinLogIterator<'s> {
    ten_min_log: Mutex<&'s mut TenMinLog>,
}

impl<'s> TenMinLogIterator<'s> {
    pub fn new(ten_min_log: &'s mut TenMinLog) -> Self {
        Self {
            ten_min_log: Mutex::new(ten_min_log),
        }
    }
    pub async fn get_next(&'s self) -> Option<LogEventFileGrpcModel> {
        let mut content_size_buf = [0u8; 4];
        let mut ten_min_log = self.ten_min_log.lock().await;

        let content_size = ten_min_log.file.read_exact(&mut content_size_buf).await;

        if let Err(err) = content_size {
            println!("Error: {:?}", err);
            return None;
        }

        let content_size = u32::from_be_bytes(content_size_buf) as usize;

        println!("{}", content_size);

        let mut event_payload = Vec::with_capacity(content_size);
        unsafe {
            event_payload.set_len(content_size);
        }

        ten_min_log
            .file
            .read_exact(&mut event_payload)
            .await
            .unwrap();

        let log_event: LogEventFileGrpcModel =
            prost::Message::decode(event_payload.as_slice()).unwrap();

        Some(log_event)
    }
}
