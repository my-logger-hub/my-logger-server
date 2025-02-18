use std::{collections::BTreeMap, sync::Arc};

use rust_extensions::{
    date_time::{DateTimeAsMicroseconds, DayKey, HourKey, IntervalKey},
    file_utils::FilePath,
};
use tokio::sync::RwLock;

use crate::log_item::LogEvent;

use super::{persist::HourlyStatisticsFileContract, HourlyStatisticsModel};

pub struct HourStatisticsRepoInner {
    items: BTreeMap<IntervalKey<HourKey>, BTreeMap<String, HourlyStatisticsModel>>,
    has_to_write: bool,
}

impl Default for HourStatisticsRepoInner {
    fn default() -> Self {
        Self {
            items: Default::default(),
            has_to_write: true,
        }
    }
}

impl HourStatisticsRepoInner {
    pub fn as_file_model(&self) -> BTreeMap<i64, BTreeMap<String, HourlyStatisticsFileContract>> {
        let mut result = BTreeMap::new();

        for (key, items_by_app) in self.items.iter() {
            let mut by_app = BTreeMap::new();
            for (app, data) in items_by_app {
                by_app.insert(app.to_string(), data.into());
            }

            result.insert(key.to_i64(), by_app);
        }

        result
    }

    pub fn from_file_model(
        src: BTreeMap<i64, BTreeMap<String, HourlyStatisticsFileContract>>,
    ) -> Self {
        let mut items = BTreeMap::new();
        for (hour_key, data) in src {
            let mut by_hour: BTreeMap<String, HourlyStatisticsModel> = BTreeMap::new();

            for (app_name, file_contract) in data {
                by_hour.insert(app_name, file_contract.into());
            }

            items.insert(hour_key.into(), by_hour);
        }

        Self {
            items,
            has_to_write: false,
        }
    }
}

pub struct HourStatisticsRepo {
    file_path: FilePath,
    inner: RwLock<BTreeMap<IntervalKey<DayKey>, HourStatisticsRepoInner>>,
}

impl HourStatisticsRepo {
    pub async fn new(file_path: FilePath) -> Self {
        let mut inner = BTreeMap::new();

        let mut now = DateTimeAsMicroseconds::now();
        let day_key: IntervalKey<DayKey> = now.into();
        if let Some(file_model) = super::persist::load(file_path.clone(), day_key).await {
            inner.insert(
                day_key,
                HourStatisticsRepoInner::from_file_model(file_model),
            );
        }

        now.add_days(-1);
        let day_key: IntervalKey<DayKey> = now.into();
        if let Some(file_model) = super::persist::load(file_path.clone(), day_key).await {
            inner.insert(
                day_key,
                HourStatisticsRepoInner::from_file_model(file_model),
            );
        }

        Self {
            file_path,
            inner: RwLock::new(inner),
        }
    }

    pub async fn new_events(&self, log_items: &[Arc<LogEvent>]) {
        let mut inner_access = self.inner.write().await;

        for log_item in log_items {
            if let Some(application) = log_item.get_application() {
                let hour_key: IntervalKey<HourKey> = log_item.timestamp.into();
                println!("HourKey: {:?}", hour_key);

                let day_key: IntervalKey<DayKey> = log_item.timestamp.into();
                println!("DayKey: {:?}", day_key);

                let by_day = match inner_access.get_mut(&day_key) {
                    Some(by_day) => by_day,
                    None => {
                        let by_day = HourStatisticsRepoInner::default();
                        inner_access.insert(day_key, by_day);
                        inner_access.get_mut(&day_key).unwrap()
                    }
                };

                by_day.has_to_write = true;

                let add_new = match by_day.items.get_mut(&hour_key) {
                    Some(by_hour_key) => {
                        if let Some(by_app) = by_hour_key.get_mut(application) {
                            by_app.inc(log_item.level);
                            false
                        } else {
                            true
                        }
                    }
                    None => true,
                };

                if add_new {
                    let mut by_hour_key = BTreeMap::new();

                    by_hour_key.insert(
                        application.to_string(),
                        HourlyStatisticsModel::from_log_item(log_item),
                    );

                    by_day.items.insert(hour_key, by_hour_key);
                }
            }
        }
    }

    pub async fn persist(&self) {
        let mut to_write = BTreeMap::new();
        let read_access = self.inner.read().await;
        for (key, value) in read_access.iter() {
            if value.has_to_write {
                to_write.insert(*key, value.as_file_model());
            }
        }

        for (date_key, data_to_persist) in to_write {
            super::persist::save(self.file_path.clone(), date_key, &data_to_persist).await;
        }
    }

    pub async fn gc(&self) {
        let mut write_access = self.inner.write().await;

        if write_access.len() > 2 {
            let key = write_access.keys().next().cloned();
            if let Some(key) = key {
                write_access.remove(&key);
            }
        }
    }

    pub async fn get_highest_and_below(
        &self,
        amount: usize,
    ) -> BTreeMap<IntervalKey<HourKey>, BTreeMap<String, HourlyStatisticsModel>> {
        let read_access = self.inner.read().await;
        let mut result = BTreeMap::new();

        for by_day in read_access.values().rev() {
            for (hour_key, items) in by_day.items.iter().rev() {
                result.insert(*hour_key, items.clone());

                if result.len() >= amount {
                    return result;
                }
            }
        }

        result
    }

    pub async fn get(
        &self,
        from_hour: IntervalKey<HourKey>,
        to_hour: IntervalKey<HourKey>,
    ) -> BTreeMap<IntervalKey<HourKey>, BTreeMap<String, HourlyStatisticsModel>> {
        let read_access = self.inner.read().await;

        let mut result = BTreeMap::new();

        for by_day in read_access.values() {
            for (hour_key, items) in by_day.items.iter() {
                if from_hour.to_i64() <= hour_key.to_i64() && hour_key.to_i64() <= to_hour.to_i64()
                {
                    result.insert(*hour_key, items.clone());
                }
            }
        }

        result
    }
}
