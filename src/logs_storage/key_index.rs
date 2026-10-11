use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use rust_extensions::{ParseUInt32VariableSizeResult, UInt32VariableSize};

use crate::repo::dto::LogLevelDto;

use super::{read_bytes, DataSource, LogRecordProto, SearchFilter, READ_LEN, SECONDS_PER_HOUR};

/// The key the level of a record is counted under: a context key does not start with '@'
pub const LEVEL_KEY: &str = "@level";

/// The process of a record is counted as the context key `Process` - the way a search
/// matches it
pub const PROCESS_KEY: &str = "process";

/// A key with more values than this in an hour is not indexed in it: a request id, an
/// account id - such an index would grow as large as the data
const MAX_VALUES_PER_KEY: usize = 1000;

/// `total_records u64 · directory_len u32 · format u8`
const HEADER_LEN: usize = 13;

/// Seconds packed: varints of the step from the second before and of the records
const FORMAT: u8 = 1;

pub fn level_value(level: &LogLevelDto) -> &'static str {
    match level {
        LogLevelDto::Info => "info",
        LogLevelDto::Warning => "warning",
        LogLevelDto::Error => "error",
        LogLevelDto::FatalError => "fatalerror",
        LogLevelDto::Debug => "debug",
    }
}

/// For each value of each key indexed in an hour, how many records of each second have
/// it. Keys and values are lowercase, as a search matches them ignoring case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyIndex {
    /// key → value → (second, records), by second
    keys: BTreeMap<String, BTreeMap<String, Vec<(u16, u32)>>>,
    /// The records counted: the counts of the index of the hour add up to it when the two
    /// agree
    total_records: u64,
}

impl KeyIndex {
    /// An index of the level, the process and of `keys`
    pub fn new(keys: &[String]) -> Self {
        let mut result = Self::default();

        for key in [LEVEL_KEY, PROCESS_KEY]
            .into_iter()
            .chain(keys.iter().map(String::as_str))
        {
            result.keys.insert(key.to_lowercase(), BTreeMap::new());
        }

        result
    }

    pub fn total_records(&self) -> u64 {
        self.total_records
    }

    /// The records of the second with the value of the key; `None` - the key is not
    /// indexed in the hour
    #[cfg(test)]
    pub fn count(&self, key: &str, value: &str, second: usize) -> Option<u32> {
        let by_value = self.keys.get(key)?;

        let count = by_value.get(value).map_or(0, |seconds| {
            seconds
                .binary_search_by_key(&(second as u16), |(second, _)| *second)
                .map_or(0, |index| seconds[index].1)
        });

        Some(count)
    }

    pub fn add_records(&mut self, second: usize, records: &[LogRecordProto]) {
        self.total_records += records.len() as u64;

        let mut values: Vec<(String, String)> = Vec::new();

        for record in records {
            values.clear();
            values.push((
                LEVEL_KEY.to_string(),
                level_value(&record.get_level()).to_string(),
            ));

            if let Some(process) = &record.process {
                values.push((PROCESS_KEY.to_string(), process.to_lowercase()));
            }

            for ctx in &record.ctx {
                let key = ctx.key.to_lowercase();

                if self.keys.contains_key(&key) {
                    values.push((key, ctx.value.to_lowercase()));
                }
            }

            values.sort();
            values.dedup();

            for (key, value) in values.drain(..) {
                self.add(key, value, second);
            }
        }
    }

    fn add(&mut self, key: String, value: String, second: usize) {
        let Some(by_value) = self.keys.get_mut(&key) else {
            return;
        };

        if !by_value.contains_key(&value) && by_value.len() >= MAX_VALUES_PER_KEY {
            // Too many values: the key is not indexed in this hour any more
            self.keys.remove(&key);
            return;
        }

        let seconds = by_value.entry(value).or_default();

        match seconds.binary_search_by_key(&(second as u16), |(second, _)| *second) {
            Ok(index) => seconds[index].1 += 1,
            Err(index) => seconds.insert(index, (second as u16, 1)),
        }
    }

    /// `total_records u64 · directory_len u32 · format u8`, then the directory - for each
    /// key its name and for each value of it the value, and the offset of its seconds from
    /// the end of the directory and their length, varints both - then the seconds of every
    /// value, packed: `varint(second - the second before) · varint(records)` each, the
    /// first second stepped from 0.
    pub fn to_file(&self) -> Vec<u8> {
        let mut directory = Vec::new();
        let mut seconds_bytes = Vec::new();

        write_len(&mut directory, self.keys.len());

        for (key, by_value) in &self.keys {
            write_string(&mut directory, key);
            write_len(&mut directory, by_value.len());

            for (value, seconds) in by_value {
                let offset = seconds_bytes.len();
                pack_seconds(seconds, &mut seconds_bytes);

                write_string(&mut directory, value);
                write_len(&mut directory, offset);
                write_len(&mut directory, seconds_bytes.len() - offset);
            }
        }

        let mut result = Vec::with_capacity(HEADER_LEN + directory.len() + seconds_bytes.len());
        result.extend_from_slice(&self.total_records.to_le_bytes());
        result.extend_from_slice(&(directory.len() as u32).to_le_bytes());
        result.push(FORMAT);
        result.extend_from_slice(&directory);
        result.extend_from_slice(&seconds_bytes);
        result
    }

    /// `None` when the bytes are not an index of this format
    pub fn from_file(bytes: &[u8]) -> Option<Self> {
        let end = KeyIndexDirectory::end_of(bytes)?;
        let total_records = u64::from_le_bytes(bytes[0..8].try_into().unwrap());

        let directory = bytes.get(HEADER_LEN..end)?;
        let seconds_bytes = &bytes[end..];

        let mut reader = Reader {
            bytes: directory,
            position: 0,
        };

        let mut keys = BTreeMap::new();

        for _ in 0..reader.read_len()? {
            let key = reader.read_string()?;
            let mut by_value = BTreeMap::new();

            for _ in 0..reader.read_len()? {
                let value = reader.read_string()?;
                let offset = reader.read_len()?;
                let len = reader.read_len()?;

                let block = seconds_bytes.get(offset..offset.checked_add(len)?)?;
                by_value.insert(value, unpack_seconds(block)?);
            }

            keys.insert(key, by_value);
        }

        Some(Self {
            keys,
            total_records,
        })
    }
}

/// Where the packed seconds of a value are in the file of a key index, and their length
#[derive(Debug, Clone, Copy)]
struct SecondsBlock {
    offset: u64,
    len: u32,
}

impl SecondsBlock {
    fn end(&self) -> u64 {
        self.offset + self.len as u64
    }
}

/// The key index of an hour as a search reads it, from a file or from S3 alike: its page
/// of every key and value in memory, the seconds of a value read when a filter asks for it
/// - 64 KiB a read at least, the blocks close to each other in one - and kept.
pub struct KeyIndexDirectory {
    keys: BTreeMap<String, BTreeMap<String, SecondsBlock>>,
    total_records: u64,
    loaded: Mutex<BTreeMap<(String, String), Arc<Vec<(u16, u32)>>>>,
    /// What the read of the directory brought in past it: the seconds at the start of the
    /// file, read with the directory - the whole index, when it is shorter than one read
    read_ahead: Vec<u8>,
    /// Where `read_ahead` is in the file
    read_ahead_start: u64,
}

impl KeyIndexDirectory {
    /// No key indexed: every search of the hour scans it
    pub fn empty() -> Self {
        Self {
            keys: BTreeMap::new(),
            total_records: 0,
            loaded: Mutex::new(BTreeMap::new()),
            read_ahead: Vec::new(),
            read_ahead_start: 0,
        }
    }

    pub fn total_records(&self) -> u64 {
        self.total_records
    }

    /// Where the directory ends in the file: read off its header. `None` while the header
    /// is cut, or when it is not of this format.
    fn end_of(header: &[u8]) -> Option<usize> {
        if header.len() < HEADER_LEN || header[12] != FORMAT {
            return None;
        }

        let directory_len = u32::from_le_bytes(header[8..12].try_into().unwrap());
        Some(HEADER_LEN + directory_len as usize)
    }

    /// `bytes` start with the header and hold the directory whole; what follows it is kept
    fn parse(bytes: &[u8]) -> Option<Self> {
        let total_records = u64::from_le_bytes(bytes.get(0..8)?.try_into().unwrap());
        let end = Self::end_of(bytes)?;

        let mut reader = Reader {
            bytes: bytes.get(HEADER_LEN..end)?,
            position: 0,
        };

        let mut keys = BTreeMap::new();

        for _ in 0..reader.read_len()? {
            let key = reader.read_string()?;
            let mut by_value = BTreeMap::new();

            for _ in 0..reader.read_len()? {
                let value = reader.read_string()?;
                let offset = reader.read_len()? as u64;
                let len = reader.read_len()? as u32;

                by_value.insert(
                    value,
                    SecondsBlock {
                        offset: end as u64 + offset,
                        len,
                    },
                );
            }

            keys.insert(key, by_value);
        }

        Some(Self {
            keys,
            total_records,
            loaded: Mutex::new(BTreeMap::new()),
            read_ahead: bytes[end..].to_vec(),
            read_ahead_start: end as u64,
        })
    }

    /// The seconds of the block, when the read of the directory brought them in
    fn read_ahead_of(&self, block: &SecondsBlock) -> Option<&[u8]> {
        let from = usize::try_from(block.offset.checked_sub(self.read_ahead_start)?).ok()?;
        self.read_ahead.get(from..from + block.len as usize)
    }

    /// The directory of a local file, read the same way; `None` when it does not read
    pub fn read_local(path: &std::path::Path) -> Option<Self> {
        use std::io::Read;

        let mut file = std::fs::File::open(path).ok()?;
        let mut bytes = vec![0u8; READ_LEN];
        let mut read = 0;

        while read < bytes.len() {
            match file.read(&mut bytes[read..]).ok()? {
                0 => break,
                size => read += size,
            }
        }

        bytes.truncate(read);

        let end = Self::end_of(&bytes)?;

        if end > bytes.len() {
            let start = bytes.len();
            bytes.resize(end, 0);
            file.read_exact(&mut bytes[start..]).ok()?;
        }

        Self::parse(&bytes)
    }

    /// The directory of the file: one read of 64 KiB, and the rest of it when it is longer
    pub async fn read(source: &DataSource) -> Result<Self, String> {
        let mut bytes = read_bytes(source, 0, READ_LEN).await?;

        let end = Self::end_of(&bytes).ok_or_else(|| "The key index is cut".to_string())?;

        if end > bytes.len() {
            let rest = read_bytes(source, bytes.len() as u64, end - bytes.len()).await?;
            bytes.extend_from_slice(&rest);
        }

        Self::parse(&bytes).ok_or_else(|| "The directory of the key index does not read".to_string())
    }

    /// The seconds of each of the values - empty for a value the hour has not got. The
    /// ones not read yet are taken from what the read of the directory brought in, or read:
    /// blocks less than one read apart in one pass.
    async fn seconds_of(
        &self,
        source: &DataSource,
        values: &[(String, String)],
    ) -> Result<Vec<Arc<Vec<(u16, u32)>>>, String> {
        let mut to_read: Vec<((String, String), SecondsBlock)> = Vec::new();

        {
            let mut loaded = self.loaded.lock().unwrap();

            for (key, value) in values {
                let value = (key.clone(), value.clone());

                if loaded.contains_key(&value) {
                    continue;
                }

                let Some(block) = self.keys.get(key).and_then(|by_value| by_value.get(&value.1))
                else {
                    continue;
                };

                match self.read_ahead_of(block) {
                    Some(bytes) => {
                        let seconds = unpack_seconds(bytes).ok_or_else(seconds_do_not_read)?;
                        loaded.insert(value, Arc::new(seconds));
                    }
                    None => to_read.push((value, *block)),
                }
            }
        }

        to_read.sort_by_key(|(_, block)| block.offset);

        let mut index = 0;

        while index < to_read.len() {
            // The blocks of one read
            let start = to_read[index].1.offset;
            let mut end = to_read[index].1.end();
            let mut last = index;

            while last + 1 < to_read.len() && to_read[last + 1].1.offset < end + READ_LEN as u64 {
                last += 1;
                end = end.max(to_read[last].1.end());
            }

            let len = ((end - start) as usize).max(READ_LEN);
            let bytes = read_bytes(source, start, len).await?;

            let mut loaded = self.loaded.lock().unwrap();

            for (value, block) in &to_read[index..=last] {
                let from = (block.offset - start) as usize;
                let to = (block.end() - start) as usize;

                let block_bytes = bytes
                    .get(from..to)
                    .ok_or_else(|| "The key index ends inside the seconds of a value".to_string())?;

                let seconds = unpack_seconds(block_bytes).ok_or_else(seconds_do_not_read)?;
                loaded.insert(value.clone(), Arc::new(seconds));
            }

            index = last + 1;
        }

        let loaded = self.loaded.lock().unwrap();

        Ok(values
            .iter()
            .map(|value| loaded.get(value).cloned().unwrap_or_default())
            .collect())
    }
}

fn seconds_do_not_read() -> String {
    "The seconds of a value of the key index do not read".to_string()
}

/// `varint(second - the second before) · varint(records)` for each second, by second
fn pack_seconds(seconds: &[(u16, u32)], out: &mut Vec<u8>) {
    let mut before = 0;

    for (second, records) in seconds {
        write_len(out, (second - before) as usize);
        write_len(out, *records as usize);
        before = *second;
    }
}

fn unpack_seconds(bytes: &[u8]) -> Option<Vec<(u16, u32)>> {
    let mut reader = Reader { bytes, position: 0 };
    let mut result = Vec::new();
    let mut second = 0;

    while reader.position < bytes.len() {
        second += reader.read_len()?;
        let records = reader.read_len()? as u32;

        if second >= SECONDS_PER_HOUR {
            return None;
        }

        result.push((second as u16, records));
    }

    Some(result)
}

/// The key index of an hour as a search uses it
#[derive(Clone)]
pub enum HourKeyIndex {
    /// The hour is written into: the whole index in memory, kept up to date by the writes
    Full(Arc<KeyIndex>),
    /// The hour is read: the directory in memory, the seconds read from `source` when asked
    Directory {
        directory: Arc<KeyIndexDirectory>,
        source: DataSource,
    },
}

impl HourKeyIndex {
    /// For each condition of the filter on indexed keys, how many records of each second
    /// meet it. A condition on a key the hour does not index is left out: it narrows
    /// nothing.
    pub async fn counts_of(&self, filter: &SearchFilter) -> FilterCounts {
        let mut result = FilterCounts {
            conditions: Vec::new(),
        };

        for condition in filter.conditions() {
            match self.condition_counts(&condition).await {
                Ok(Some(counts)) => result.conditions.push(counts),
                Ok(None) => {}
                Err(err) => println!("The key index does not read: {}", err),
            }
        }

        result
    }

    /// The records of each second with any of the `key=value`s
    async fn condition_counts(
        &self,
        condition: &[(String, String)],
    ) -> Result<Option<Vec<u32>>, String> {
        let mut counts = vec![0u32; SECONDS_PER_HOUR];

        match self {
            HourKeyIndex::Full(index) => {
                for (key, value) in condition {
                    let Some(by_value) = index.keys.get(key) else {
                        return Ok(None);
                    };

                    for (second, records) in by_value.get(value).into_iter().flatten() {
                        counts[*second as usize] += records;
                    }
                }
            }
            HourKeyIndex::Directory { directory, source } => {
                if condition
                    .iter()
                    .any(|(key, _)| !directory.keys.contains_key(key))
                {
                    return Ok(None);
                }

                for seconds in directory.seconds_of(source, condition).await? {
                    for (second, records) in seconds.iter() {
                        counts[*second as usize] += records;
                    }
                }
            }
        }

        Ok(Some(counts))
    }
}

/// What the key index says of a filter, second by second
pub struct FilterCounts {
    conditions: Vec<Vec<u32>>,
}

impl FilterCounts {
    /// At most how many of the `records` of the second match: exact for one condition
    pub fn estimate(&self, records: u32, second: usize) -> u32 {
        self.conditions
            .iter()
            .fold(records, |result, counts| result.min(counts[second]))
    }
}

fn write_len(out: &mut Vec<u8>, len: usize) {
    UInt32VariableSize::new(len as u32).serialize(out);
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_len(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

struct Reader<'s> {
    bytes: &'s [u8],
    position: usize,
}

impl<'s> Reader<'s> {
    fn read_len(&mut self) -> Option<usize> {
        let rest = self.bytes.get(self.position..).filter(|rest| !rest.is_empty())?;

        match UInt32VariableSize::from_slice(rest) {
            ParseUInt32VariableSizeResult::Ok { value, size } => {
                self.position += size;
                Some(value.get_value() as usize)
            }
            ParseUInt32VariableSizeResult::NotEnoughDataInBuffer(_) => None,
        }
    }

    fn read_string(&mut self) -> Option<String> {
        let len = self.read_len()?;
        let bytes = self.bytes.get(self.position..self.position + len)?;
        self.position += len;
        String::from_utf8(bytes.to_vec()).ok()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs_storage::LogRecordCtxProto;

    fn record(level: i32, application: &str, trader: &str) -> LogRecordProto {
        LogRecordProto {
            timestamp: 0,
            level,
            process: Some("Payments".to_string()),
            message: "message".to_string(),
            ctx: vec![
                LogRecordCtxProto {
                    key: "Application".to_string(),
                    value: application.to_string(),
                },
                LogRecordCtxProto {
                    key: "TraderId".to_string(),
                    value: trader.to_string(),
                },
            ],
        }
    }

    #[test]
    fn counts_the_values_of_the_indexed_keys() {
        let mut index = KeyIndex::new(&["Application".to_string()]);

        index.add_records(5, &[record(2, "Billing", "1"), record(1, "billing", "2")]);
        index.add_records(7, &[record(2, "Trading", "3")]);

        assert_eq!(index.count("application", "billing", 5), Some(2));
        assert_eq!(index.count("application", "billing", 7), Some(0));
        assert_eq!(index.count("application", "trading", 7), Some(1));
        assert_eq!(index.count(LEVEL_KEY, "error", 5), Some(1));
        assert_eq!(index.count(LEVEL_KEY, "warning", 5), Some(1));
        assert_eq!(index.count(PROCESS_KEY, "payments", 7), Some(1));
        // Not indexed
        assert_eq!(index.count("traderid", "1", 5), None);
        assert_eq!(index.total_records(), 3);

        assert_eq!(KeyIndex::from_file(&index.to_file()), Some(index));
    }

    #[tokio::test]
    async fn seconds_past_the_first_read_are_read_from_the_file() {
        let mut index = KeyIndex::new(&["TraderId".to_string()]);

        // 900 traders in each of 120 seconds: their seconds take more than two reads
        for second in 0..120 {
            let records: Vec<LogRecordProto> = (0..900)
                .map(|no| record(2, "billing", &no.to_string()))
                .collect();
            index.add_records(second, &records);
        }

        let path = std::env::temp_dir().join(format!(
            "my-logger-server-key-index-{}",
            std::process::id()
        ));
        std::fs::write(&path, index.to_file()).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 2 * READ_LEN as u64);

        let key_index = HourKeyIndex::Directory {
            directory: Arc::new(KeyIndexDirectory::read_local(&path).unwrap()),
            source: DataSource::Local(path.clone()),
        };

        // The first one is in the read of the page, the others past it
        for trader in ["0", "450", "899"] {
            let filter = SearchFilter::new(
                None,
                Some(BTreeMap::from([("traderid".to_string(), trader.to_string())])),
                None,
            );
            let counts = key_index.counts_of(&filter).await;

            for second in 0..120 {
                assert_eq!(counts.estimate(u32::MAX, second), 1, "trader {}", trader);
            }
            assert_eq!(counts.estimate(u32::MAX, 120), 0);
        }

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn seconds_are_packed() {
        let mut index = KeyIndex::new(&[]);

        // A process logging every second of the hour, up to 9 records a second
        for second in 0..SECONDS_PER_HOUR {
            let records: Vec<LogRecordProto> = (0..1 + second % 9)
                .map(|_| record(2, "billing", "1"))
                .collect();
            index.add_records(second, &records);
        }

        // Two values - the level and the process - in 2 bytes a second each, not 6
        let file = index.to_file();
        assert!(file.len() < 2 * SECONDS_PER_HOUR * 2 + 100, "{} bytes", file.len());
        assert_eq!(KeyIndex::from_file(&file), Some(index));
    }

    #[test]
    fn a_key_with_too_many_values_is_not_indexed() {
        let mut index = KeyIndex::new(&["TraderId".to_string()]);

        let records: Vec<LogRecordProto> = (0..MAX_VALUES_PER_KEY + 1)
            .map(|no| record(2, "billing", &no.to_string()))
            .collect();
        index.add_records(1, &records);

        assert_eq!(index.count("traderid", "1", 1), None);
        assert_eq!(index.count(PROCESS_KEY, "payments", 1), Some(records.len() as u32));
    }
}
