use std::collections::BTreeMap;

use rust_extensions::{
    date_time::{DayKey, IntervalKey},
    file_utils::FilePath,
};
use serde::{Deserialize, Serialize};

use super::HourlyStatisticsModel;

pub async fn save(
    file_path: FilePath,
    day_key: IntervalKey<DayKey>,
    data_to_persist: &BTreeMap<i64, BTreeMap<String, HourlyStatisticsFileContract>>,
) {
    let file_path = compile_file_name(file_path, day_key);

    let data = serde_yaml::to_string(&data_to_persist).unwrap();
    tokio::fs::write(file_path.as_str(), data.as_bytes())
        .await
        .unwrap();
}

pub async fn load(
    file_path: FilePath,
    day_key: IntervalKey<DayKey>,
) -> Option<BTreeMap<i64, BTreeMap<String, HourlyStatisticsFileContract>>> {
    let file_path = compile_file_name(file_path, day_key);

    let file_content = tokio::fs::read(file_path.as_str()).await;

    let file_content = match file_content {
        Ok(file_content) => file_content,
        Err(err) => {
            println!(
                "Skipping statistic data because can not read statistic data from: {}. Err: {}",
                file_path.as_str(),
                err
            );
            return None;
        }
    };

    let file_data: Result<
        BTreeMap<i64, BTreeMap<String, HourlyStatisticsFileContract>>,
        serde_yaml::Error,
    > = serde_yaml::from_slice(file_content.as_slice());

    let file_data = match file_data {
        Ok(file_data) => file_data,
        Err(_) => {
            println!(
                "Can not deserialize hour statistics data from file {}",
                file_path.as_str()
            );

            return None;
        }
    };

    let mut result = BTreeMap::new();

    for (key, value) in file_data {
        result.insert(key.into(), value.into());
    }

    Some(result)
}
fn compile_file_name(mut file_path: FilePath, day_key: IntervalKey<DayKey>) -> FilePath {
    let file_name = format!("hour-stat-{}", day_key.to_i64());

    file_path.append_segment(&file_name);
    file_path
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HourlyStatisticsFileContract {
    pub info: u32,
    pub warning: u32,
    pub error: u32,
    pub fatal_error: u32,
    pub debug: u32,
}

impl Into<HourlyStatisticsFileContract> for &'_ HourlyStatisticsModel {
    fn into(self) -> HourlyStatisticsFileContract {
        HourlyStatisticsFileContract {
            info: self.info,
            warning: self.warning,
            error: self.error,
            fatal_error: self.fatal_error,
            debug: self.debug,
        }
    }
}

impl Into<HourlyStatisticsModel> for HourlyStatisticsFileContract {
    fn into(self) -> HourlyStatisticsModel {
        HourlyStatisticsModel {
            info: self.info,
            warning: self.warning,
            error: self.error,
            fatal_error: self.fatal_error,
            debug: self.debug,
        }
    }
}
