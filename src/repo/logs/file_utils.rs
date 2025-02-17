use rust_extensions::{date_time::DateTimeAsMicroseconds, file_utils::FilePath};

use super::TenMinKey;

const LOGS_PREFIX: &'static str = "logs-";
pub fn compile_file_name(db_path: &FilePath, ten_min_key: TenMinKey) -> FilePath {
    let mut file_path = db_path.clone();

    let file_name = format!("{LOGS_PREFIX}{}", ten_min_key.as_u64());
    file_path.append_segment(file_name.as_str());
    file_path
}

#[derive(Debug)]
pub struct MinMax {
    pub min: u64,
    pub max: u64,
}

impl MinMax {
    pub fn update(&mut self, value: u64) {
        if value < self.min {
            self.min = value;
        }

        if value > self.max {
            self.max = value;
        }
    }
}

pub async fn gc_files(db_path: &FilePath, from: DateTimeAsMicroseconds) -> Option<MinMax> {
    let files = get_logs_files_from_dir(db_path).await;

    let mut result: Option<MinMax> = None;

    for file_name in files {
        let ten_min_key = &file_name[LOGS_PREFIX.len()..];

        let ten_min_key: Result<u64, _> = ten_min_key.parse();

        if ten_min_key.is_err() {
            println!("Invalid file {} to execute gc", file_name.as_str());
            continue;
        }

        let ten_min_key = ten_min_key.unwrap();

        match result.as_mut() {
            Some(itm) => {
                itm.update(ten_min_key);
            }
            None => {
                result = MinMax {
                    min: ten_min_key,
                    max: ten_min_key,
                }
                .into()
            }
        }

        let ten_min_key: TenMinKey = ten_min_key.into();

        let date: DateTimeAsMicroseconds = ten_min_key.into();

        if date <= from {
            println!("Garbage collecting logs file: {}", file_name);
            let err = tokio::fs::remove_file(&file_name).await;
            if let Err(err) = err {
                println!("Can not GC file {}. Error: {}", file_name, err);
            }
        }
    }

    result
}

pub async fn get_logs_files_from_dir(db_path: &FilePath) -> Vec<String> {
    let mut result = Vec::new();
    let read_dir = tokio::fs::read_dir(db_path.as_str()).await;

    let mut read_dir = match read_dir {
        Ok(result) => result,
        Err(err) => {
            println!(
                "GC of logs at path: [{}] finished with error. Err: {}",
                db_path.as_str(),
                err
            );
            return vec![];
        }
    };

    loop {
        let next_entry = read_dir.next_entry().await;

        let next_entity = match next_entry {
            Ok(value) => value,
            Err(err) => {
                println!(
                    "Can not get next file at path {}. Err: {:?}",
                    db_path.as_str(),
                    err
                );
                return vec![];
            }
        };

        if next_entity.is_none() {
            break;
        }

        let next_entity = next_entity.unwrap();

        let file_type = match next_entity.file_type().await {
            Ok(file_type) => file_type,
            Err(err) => {
                panic!(
                    "Can not get file type of file [{:?}]. Err: {}",
                    next_entity.file_name(),
                    err
                );
            }
        };

        if !file_type.is_file() {
            continue;
        }

        let file_name = next_entity.file_name().into_string().unwrap();

        if file_name.starts_with(LOGS_PREFIX) {
            result.push(file_name);
        }
    }

    result
}
