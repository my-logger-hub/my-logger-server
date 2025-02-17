use my_logger::LogLevel;

use crate::log_item::LogEvent;

#[derive(Debug, Clone)]
pub struct IgnoreEventModel {
    pub level: LogLevel,
    pub application: String,
    pub marker: String,
}

impl IgnoreEventModel {
    pub fn is_same(&self, other: &Self) -> bool {
        self.marker == other.marker
            && self.application == other.application
            && self.level.to_u8() == other.level.to_u8()
    }

    pub fn ignore_me(&self, log_event: &LogEvent) -> bool {
        if !log_event.level.eq_to(&self.level) {
            return false;
        }

        if let Some(application) = log_event.application.as_ref() {
            if application.as_str() != self.application {
                return false;
            }
        }

        log_event.message.contains(self.marker.as_str())
    }
}
