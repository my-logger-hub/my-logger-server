use rust_extensions::date_time::{DateTimeAsMicroseconds, DateTimeStruct};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatisticsHour(u64);

impl StatisticsHour {
    pub fn get_value(&self) -> u64 {
        self.0
    }
}

impl Into<StatisticsHour> for DateTimeAsMicroseconds {
    fn into(self) -> StatisticsHour {
        let dts: DateTimeStruct = self.into();

        let key = dts.year as u64 * 1000000
            + dts.month as u64 * 10000
            + dts.day as u64 * 100
            + dts.time.hour as u64;
        StatisticsHour(key)
    }
}

impl Into<StatisticsHour> for u64 {
    fn into(self) -> StatisticsHour {
        StatisticsHour(self)
    }
}

impl Into<StatisticsHour> for i64 {
    fn into(self) -> StatisticsHour {
        StatisticsHour(self as u64)
    }
}
