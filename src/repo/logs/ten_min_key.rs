use std::time::Duration;

use rust_extensions::date_time::{DateTimeAsMicroseconds, DateTimeStruct, TimeStruct};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct TenMinKey(u64);

impl TenMinKey {
    pub fn new(src: DateTimeAsMicroseconds) -> Self {
        let dt_struct: DateTimeStruct = src.into();

        let min = dt_struct.time.min / 10;

        let result = dt_struct.year as u64 * 100000000
            + dt_struct.month as u64 * 1000000
            + dt_struct.day as u64 * 10000
            + dt_struct.time.hour as u64 * 100
            + min as u64 * 10;
        Self(result)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn inc(&mut self) {
        let dt: DateTimeAsMicroseconds = (*self).into();

        let dt = dt.add(Duration::from_secs(60 * 10));

        let result: TenMinKey = dt.into();

        self.0 = result.0;
    }
}

impl Into<DateTimeAsMicroseconds> for TenMinKey {
    fn into(self) -> DateTimeAsMicroseconds {
        let mut value = self.0;

        println!("{}", value);

        let year = value / 100000000;
        value = value - (year * 100000000);

        let month = value / 1000000;
        value = value - (month * 1000000);

        let day = value / 10000;
        value = value - (day * 10000);

        let hour = value / 100;
        value = value - (hour * 100);

        let min = value;

        let dt_struct = DateTimeStruct {
            year: year as i32,
            month: month as u32,
            day: day as u32,
            time: TimeStruct {
                hour: hour as u32,
                min: min as u32,
                sec: 0,
                micros: 0,
            },
            dow: None,
        };

        dt_struct.try_into().unwrap()
    }
}

impl Into<TenMinKey> for DateTimeAsMicroseconds {
    fn into(self) -> TenMinKey {
        TenMinKey::new(self)
    }
}

impl Into<TenMinKey> for u64 {
    fn into(self) -> TenMinKey {
        TenMinKey(self)
    }
}

#[cfg(test)]
mod tests {
    use rust_extensions::date_time::DateTimeAsMicroseconds;

    use super::TenMinKey;

    #[test]
    fn test() {
        let dt = DateTimeAsMicroseconds::from_str("2025-01-15T12:02:00").unwrap();
        let ten_min_key: TenMinKey = dt.into();
        assert_eq!(ten_min_key.as_u64(), 202501151200);

        let dt_back: DateTimeAsMicroseconds = ten_min_key.into();

        let result = dt_back.to_rfc3339();
        assert_eq!(&result[..19], "2025-01-15T12:00:00");

        let dt = DateTimeAsMicroseconds::from_str("2025-01-15T12:10:00").unwrap();
        let ten_min_key: TenMinKey = dt.into();
        assert_eq!(ten_min_key.as_u64(), 202501151210);

        let dt_back: DateTimeAsMicroseconds = ten_min_key.into();
        let result = dt_back.to_rfc3339();
        assert_eq!(&result[..19], "2025-01-15T12:10:00");

        let dt = DateTimeAsMicroseconds::from_str("2025-01-15T12:11:00").unwrap();
        let ten_min_key: TenMinKey = dt.into();
        assert_eq!(ten_min_key.as_u64(), 202501151210);

        let dt_back: DateTimeAsMicroseconds = ten_min_key.into();
        let result = dt_back.to_rfc3339();
        assert_eq!(&result[..19], "2025-01-15T12:10:00");
    }
}
