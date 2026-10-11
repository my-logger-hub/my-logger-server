use std::collections::BTreeMap;

use prost::Message;
use rust_extensions::{
    date_time::DateTimeAsMicroseconds, ParseUInt32VariableSizeResult, UInt32VariableSize,
};

use crate::{
    app::{LogItem, PROCESS_CONTEXT_KEY},
    repo::dto::{LogItemDto, LogLevelDto},
};

use super::{encode_entry, ENTRY_TIME_LEN, MAX_ENTRY_BODY_LEN};

const ZSTD_LEVEL: i32 = 3;

/// Records are put together into a block of about this many bytes before it is
/// compressed: logs compress to a few KiB from it, which is what an entry holds.
const RAW_BLOCK_LEN: usize = 64 * 1024;

const MAX_PAYLOAD_LEN: usize = MAX_ENTRY_BODY_LEN - ENTRY_TIME_LEN;

/// What a string cut to fit an entry ends with
const TRUNCATED_MARK: &str = "...[truncated]";

/// A string shorter than this is never cut: the number of context entries is
const MIN_LEN_TO_CUT: usize = 64;

#[derive(Clone, PartialEq, prost::Message)]
pub struct LogRecordProto {
    #[prost(int64, tag = "1")]
    pub timestamp: i64,
    #[prost(int32, tag = "2")]
    pub level: i32,
    #[prost(string, optional, tag = "3")]
    pub process: Option<String>,
    #[prost(string, tag = "4")]
    pub message: String,
    #[prost(message, repeated, tag = "5")]
    pub ctx: Vec<LogRecordCtxProto>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct LogRecordCtxProto {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

impl LogRecordProto {
    pub fn from_log_item(item: &LogItem, timestamp: i64) -> Self {
        let level: LogLevelDto = (&item.level).into();

        Self {
            timestamp,
            level: level_code(&level),
            process: item.process.clone(),
            message: item.message.clone(),
            ctx: item
                .ctx
                .iter()
                .map(|(key, value)| LogRecordCtxProto {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
        }
    }

    pub fn get_level(&self) -> LogLevelDto {
        match self.level {
            1 => LogLevelDto::Warning,
            2 => LogLevelDto::Error,
            3 => LogLevelDto::FatalError,
            4 => LogLevelDto::Debug,
            _ => LogLevelDto::Info,
        }
    }

    pub fn into_dto(self) -> LogItemDto {
        let level = self.get_level();

        let mut context: BTreeMap<String, String> = self
            .ctx
            .into_iter()
            .map(|ctx| (ctx.key, ctx.value))
            .collect();

        if let Some(process) = self.process {
            context.insert(PROCESS_CONTEXT_KEY.to_string(), process);
        }

        LogItemDto {
            moment: DateTimeAsMicroseconds::new(self.timestamp),
            level,
            message: self.message,
            context,
        }
    }
}

fn level_code(level: &LogLevelDto) -> i32 {
    match level {
        LogLevelDto::Info => 0,
        LogLevelDto::Warning => 1,
        LogLevelDto::Error => 2,
        LogLevelDto::FatalError => 3,
        LogLevelDto::Debug => 4,
    }
}

/// An entry: a block of records compressed to fit it
pub struct CompressedBlock {
    /// Its time is the time of its first record
    pub entry: Vec<u8>,
    /// How many records it holds
    pub records: usize,
}

/// The records of one second, in time order, as entries: blocks of them, each
/// compressed to fit an entry.
pub fn compress_records(records: Vec<LogRecordProto>) -> Vec<CompressedBlock> {
    let mut result = Vec::new();
    let mut block = Vec::new();
    let mut block_len = 0;

    for record in records {
        let record_len = record.encoded_len();

        if !block.is_empty() && block_len + record_len > RAW_BLOCK_LEN {
            compress_block(std::mem::take(&mut block), &mut result);
            block_len = 0;
        }

        block_len += record_len;
        block.push(record);
    }

    if !block.is_empty() {
        compress_block(block, &mut result);
    }

    result
}

/// A block which does not compress to fit an entry is halved; a record which does not
/// fit alone is cut.
fn compress_block(mut records: Vec<LogRecordProto>, out: &mut Vec<CompressedBlock>) {
    let payload = zstd::bulk::compress(&encode_records(&records), ZSTD_LEVEL)
        .expect("zstd compresses a buffer in memory");

    if payload.len() <= MAX_PAYLOAD_LEN {
        let time = records[0].timestamp;
        out.push(CompressedBlock {
            entry: encode_entry(time, &payload),
            records: records.len(),
        });
        return;
    }

    if records.len() > 1 {
        let upper = records.split_off(records.len() / 2);
        compress_block(records, out);
        compress_block(upper, out);
        return;
    }

    let mut record = records.pop().unwrap();

    if shrink(&mut record) {
        compress_block(vec![record], out);
    }
}

fn encode_records(records: &[LogRecordProto]) -> Vec<u8> {
    let mut result = Vec::new();

    for record in records {
        UInt32VariableSize::new(record.encoded_len() as u32).serialize(&mut result);
        record
            .encode(&mut result)
            .expect("a Vec grows to hold a record");
    }

    result
}

/// Cuts the longest string of the record in half; with only short strings left, drops
/// half of the context. `false` - there is nothing left to cut.
fn shrink(record: &mut LogRecordProto) -> bool {
    let message_len = record.message.len();
    let process_len = record.process.as_ref().map(|p| p.len()).unwrap_or(0);
    let longest_ctx = record
        .ctx
        .iter()
        .enumerate()
        .max_by_key(|(_, ctx)| ctx.value.len())
        .map(|(index, ctx)| (index, ctx.value.len()));

    let longest = message_len
        .max(process_len)
        .max(longest_ctx.map(|(_, len)| len).unwrap_or(0));

    if longest < MIN_LEN_TO_CUT {
        if record.ctx.is_empty() {
            return false;
        }

        record.ctx.truncate(record.ctx.len() / 2);
        return true;
    }

    if message_len == longest {
        cut_in_half(&mut record.message);
    } else if process_len == longest {
        cut_in_half(record.process.as_mut().unwrap());
    } else {
        let (index, _) = longest_ctx.unwrap();
        cut_in_half(&mut record.ctx[index].value);
    }

    true
}

fn cut_in_half(value: &mut String) {
    let len = value
        .strip_suffix(TRUNCATED_MARK)
        .map(|value| value.len())
        .unwrap_or(value.len());

    let mut at = len / 2;
    while !value.is_char_boundary(at) {
        at -= 1;
    }

    value.truncate(at);
    value.push_str(TRUNCATED_MARK);
}

pub fn decompress_records(payload: &[u8]) -> Result<Vec<LogRecordProto>, String> {
    let raw = zstd::stream::decode_all(payload)
        .map_err(|err| format!("A block does not decompress: {}", err))?;

    let mut result = Vec::new();
    let mut offset = 0;

    while offset < raw.len() {
        let (len, prefix_len) = match UInt32VariableSize::from_slice(&raw[offset..]) {
            ParseUInt32VariableSizeResult::Ok { value, size } => {
                (value.get_value() as usize, size)
            }
            ParseUInt32VariableSizeResult::NotEnoughDataInBuffer(_) => {
                return Err("The length of a record of a block is cut".to_string());
            }
        };

        let start = offset + prefix_len;
        let end = start + len;

        if end > raw.len() {
            return Err("A record reaches past the end of its block".to_string());
        }

        let record = LogRecordProto::decode(&raw[start..end])
            .map_err(|err| format!("A record of a block does not decode: {}", err))?;

        result.push(record);
        offset = end;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs_storage::parse_entry;

    fn record(timestamp: i64, message: String) -> LogRecordProto {
        LogRecordProto {
            timestamp,
            level: 2,
            process: Some("process".to_string()),
            message,
            ctx: vec![LogRecordCtxProto {
                key: "Application".to_string(),
                value: "billing".to_string(),
            }],
        }
    }

    fn records_of(blocks: &[CompressedBlock]) -> Vec<LogRecordProto> {
        blocks
            .iter()
            .flat_map(|block| {
                let entry = parse_entry(&block.entry, 0).unwrap();
                let records = decompress_records(entry.payload).unwrap();
                assert_eq!(records.len(), block.records);
                records
            })
            .collect()
    }

    #[test]
    fn records_come_back_from_their_blocks() {
        let records: Vec<LogRecordProto> = (0..5000)
            .map(|no| record(1_000_000 + no, format!("Message number {}", no)))
            .collect();

        let blocks = compress_records(records.clone());

        assert!(blocks.len() > 1);
        assert!(blocks
            .iter()
            .all(|block| block.entry.len() <= MAX_ENTRY_BODY_LEN + 2));
        assert_eq!(parse_entry(&blocks[0].entry, 0).unwrap().time, 1_000_000);
        assert_eq!(records_of(&blocks), records);
    }

    #[test]
    fn a_record_too_long_for_an_entry_is_cut() {
        // Random letters compress poorly
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let message: String = (0..100_000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                char::from(b'a' + (state % 26) as u8)
            })
            .collect();

        let blocks = compress_records(vec![record(1, message.clone())]);
        let records = records_of(&blocks);

        assert_eq!(records.len(), 1);
        assert!(records[0].message.ends_with(TRUNCATED_MARK));
        assert!(message.starts_with(records[0].message.strip_suffix(TRUNCATED_MARK).unwrap()));
    }
}
