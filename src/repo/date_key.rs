use std::{collections::BTreeMap, time::Duration};

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
        mut from: DateTimeAsMicroseconds,
        to: DateTimeAsMicroseconds,
    ) -> BTreeMap<Self, ()> {
        let to_key = Self::new(to);
        let mut current = Self::new(from);

        let mut result = BTreeMap::new();
        while current <= to_key {
            result.insert(current, ());
            from = from.add(Duration::from_secs(60 * 60));
            current = Self::new(from);
        }

        result
    }
}

impl From<i64> for DateKey {
    fn from(value: i64) -> Self {
        Self(value)
    }
}
