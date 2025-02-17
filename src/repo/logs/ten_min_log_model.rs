use my_logger::LogLevel;

use crate::my_logger_grpc::*;

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LogEventCtxFileGrpcModel {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

impl Into<LogEventCtxFileGrpcModel> for LogEventContext {
    fn into(self) -> LogEventCtxFileGrpcModel {
        LogEventCtxFileGrpcModel {
            key: self.key,
            value: self.value,
        }
    }
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LogEventFileGrpcModel {
    #[prost(sint64, tag = "1")]
    pub timestamp: i64,

    #[prost(string, tag = "2")]
    pub id: String,

    #[prost(int32, tag = "3")]
    pub level: i32,

    #[prost(string, tag = "4")]
    pub message: String,

    #[prost(string, optional, tag = "5")]
    pub process: Option<String>,

    #[prost(repeated, message, tag = "6")]
    pub ctx: Vec<LogEventCtxFileGrpcModel>,
}

impl LogEventFileGrpcModel {
    pub fn remove_key(&mut self, name: &str) -> Option<LogEventCtxFileGrpcModel> {
        let index = self.ctx.iter().position(|itm| itm.key == name)?;

        let result = self.ctx.remove(index);
        Some(result)
    }

    pub fn get_log_level(&self) -> LogLevel {
        LogLevel::from_u8(self.level as u8)
    }

    pub fn filter_by_log_level(&self, levels: &[LogLevel]) -> bool {
        if levels.len() == 0 {
            return true;
        }

        for to_match in levels {
            if to_match.to_u8() == self.level as u8 {
                return true;
            }
        }

        false
    }

    fn has_ctx(&self, key: &str, value: &str) -> bool {
        for ctx in self.ctx.iter() {
            if ctx.key == key && ctx.value == value {
                return true;
            }
        }

        false
    }

    pub fn filter_by_ctx<'k, 'v>(&self, ctx_in: &[LogEventCtxFileGrpcModel]) -> bool {
        if ctx_in.len() == 0 {
            return true;
        }

        for ctx_in in ctx_in {
            if !self.has_ctx(&ctx_in.key, &ctx_in.value) {
                return false;
            }
        }

        true
    }

    pub fn filter_by_phrase(&self, phrase: &str) -> bool {
        if self.message.contains(phrase) {
            return true;
        }

        if let Some(process) = self.process.as_ref() {
            if process.contains(phrase) {
                return true;
            }
        }

        for itm in self.ctx.iter() {
            if itm.value.contains(phrase) {
                return true;
            }
        }

        false
    }
}
