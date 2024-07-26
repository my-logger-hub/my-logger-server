use std::time::Duration;

use rust_extensions::date_time::{DateTimeAsMicroseconds, DateTimeStruct};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]

pub struct DateKey(i64);

impl DateKey {
    pub fn new(now: DateTimeAsMicroseconds) -> Self {
        let itm: DateTimeStruct = now.into();

        Self(
            itm.year as i64 * 1000000
                + itm.month as i64 * 10000
                + itm.day as i64 * 100
                + itm.time.hour as i64,
        )
    }

    pub fn get_value(&self) -> i64 {
        self.0
    }

    pub fn get_keys_to_request(
        from: DateTimeAsMicroseconds,
        to: DateTimeAsMicroseconds,
    ) -> Vec<Self> {
        let from_key = Self::new(from);
        let to_key = Self::new(to);

        if from_key.0 == to_key.0 {
            return vec![from_key];
        }

        let next_date = from.add(Duration::from_secs(60 * 60));

        vec![from_key, Self::new(next_date)]
    }
}

impl From<i64> for DateKey {
    fn from(value: i64) -> Self {
        Self(value)
    }
}
