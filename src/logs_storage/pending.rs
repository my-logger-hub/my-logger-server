use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    app::LogItem,
    repo::dto::{LogItemDto, LogLevelDto},
};

use super::{LogRecordProto, SearchFilter, MICROS_PER_SECOND};

/// Records wait this long in memory before they are written: the records of a second
/// which come late by up to that go into its block with it.
pub const PENDING_DELAY: Duration = Duration::from_secs(5);

/// Past this many records the oldest seconds are dropped - a backstop for when the
/// writes fall behind
const MAX_PENDING_RECORDS: usize = 200_000;

/// A time further than this ahead of the server's is taken for a wrong one
const MAX_TIME_AHEAD_MICROS: i64 = 24 * 60 * 60 * MICROS_PER_SECOND;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogsStream {
    /// Warning, Error and FatalError: uploaded to S3 when it is set up
    Main,
    /// Debug and Info: kept on the local disk for a short while, then deleted
    DebugInfo,
}

impl LogsStream {
    pub const ALL: [LogsStream; 2] = [LogsStream::Main, LogsStream::DebugInfo];

    pub fn of(level: &LogLevelDto) -> Self {
        match level {
            LogLevelDto::Debug | LogLevelDto::Info => Self::DebugInfo,
            LogLevelDto::Warning | LogLevelDto::Error | LogLevelDto::FatalError => Self::Main,
        }
    }

    /// The directory of its files under `LogsDbPath`
    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::Main => "logs",
            Self::DebugInfo => "debug-info",
        }
    }
}

pub struct PendingRecord {
    pub timestamp: i64,
    pub item: Arc<LogItem>,
}

struct PendingBucket {
    created: Instant,
    records: Vec<PendingRecord>,
}

/// The records of one second of a stream of a product scope which have waited long enough
pub struct ReadyBucket {
    pub stream: LogsStream,
    pub scope: String,
    /// Unix seconds
    pub second: i64,
    pub records: Vec<PendingRecord>,
}

impl ReadyBucket {
    /// The records as they are stored, in time order
    pub fn into_protos(mut self) -> Vec<LogRecordProto> {
        self.records.sort_by_key(|record| record.timestamp);

        self.records
            .iter()
            .map(|record| LogRecordProto::from_log_item(&record.item, record.timestamp))
            .collect()
    }
}

/// The records waiting to be written, by stream, product scope and second. A second waits
/// `PENDING_DELAY` from its first record.
#[derive(Default)]
pub struct PendingLogs {
    buckets: BTreeMap<(LogsStream, String, i64), PendingBucket>,
    records: usize,
}

impl PendingLogs {
    /// Returns how many records were dropped to keep the buffer bounded
    pub fn add(
        &mut self,
        items: Vec<Arc<LogItem>>,
        now: Instant,
        now_micros: i64,
        scope_of: impl Fn(&LogItem) -> String,
    ) -> usize {
        for item in items {
            let timestamp = effective_timestamp(item.timestamp.unix_microseconds, now_micros);
            let level: LogLevelDto = (&item.level).into();
            let key = (
                LogsStream::of(&level),
                scope_of(&item),
                timestamp.div_euclid(MICROS_PER_SECOND),
            );

            self.buckets
                .entry(key)
                .or_insert_with(|| PendingBucket {
                    created: now,
                    records: Vec::new(),
                })
                .records
                .push(PendingRecord { timestamp, item });

            self.records += 1;
        }

        let mut dropped = 0;

        while self.records > MAX_PENDING_RECORDS {
            let oldest = self
                .buckets
                .iter()
                .min_by_key(|(_, bucket)| bucket.created)
                .map(|(key, _)| key.clone())
                .unwrap();

            let bucket = self.buckets.remove(&oldest).unwrap();
            self.records -= bucket.records.len();
            dropped += bucket.records.len();
        }

        dropped
    }

    /// The seconds which have waited `PENDING_DELAY` - or all of them
    pub fn take_ready(&mut self, now: Instant, all: bool) -> Vec<ReadyBucket> {
        let keys: Vec<(LogsStream, String, i64)> = self
            .buckets
            .iter()
            .filter(|(_, bucket)| all || now.duration_since(bucket.created) >= PENDING_DELAY)
            .map(|(key, _)| key.clone())
            .collect();

        keys.into_iter()
            .map(|key| {
                let bucket = self.buckets.remove(&key).unwrap();
                self.records -= bucket.records.len();

                let (stream, scope, second) = key;

                ReadyBucket {
                    stream,
                    scope,
                    second,
                    records: bucket.records,
                }
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.records
    }

    /// The waiting records of the scope in `from ..= to` which match
    pub fn search(
        &self,
        scope: &str,
        from: i64,
        to: i64,
        filter: &SearchFilter,
    ) -> Vec<LogItemDto> {
        let mut result = Vec::new();

        for ((stream, bucket_scope, second), bucket) in &self.buckets {
            let second_start = second * MICROS_PER_SECOND;

            if bucket_scope != scope
                || !filter.wants(*stream)
                || second_start > to
                || second_start + MICROS_PER_SECOND <= from
            {
                continue;
            }

            for record in &bucket.records {
                if record.timestamp < from || record.timestamp > to {
                    continue;
                }

                if filter.matches_item(&record.item) {
                    result.push(
                        LogRecordProto::from_log_item(&record.item, record.timestamp).into_dto(),
                    );
                }
            }
        }

        result
    }
}

/// A record with no time, or with a time a day ahead, is put at the time it came
fn effective_timestamp(timestamp: i64, now: i64) -> i64 {
    if timestamp <= 0 || timestamp > now + MAX_TIME_AHEAD_MICROS {
        now
    } else {
        timestamp
    }
}
