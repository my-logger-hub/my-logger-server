use std::{
    collections::{BTreeMap, HashMap},
    os::unix::fs::FileExt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use my_logger::LogLevel;
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{
    app::LogItem,
    repo::{
        dto::{LogItemDto, LogLevelDto},
        DateHourKey,
    },
};

use super::*;

fn indexed_keys() -> Vec<String> {
    vec!["Application".to_string()]
}

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "my-logger-server-tests-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Letters which compress poorly, so blocks are KiBs and frames fill and get halved
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, max: u64) -> u64 {
        self.next() % max
    }

    fn text(&mut self, len: usize) -> String {
        (0..len)
            .map(|_| char::from(b'a' + self.below(26) as u8))
            .collect()
    }
}

fn record(random: &mut Random, timestamp: i64) -> LogRecordProto {
    let len = 20 + random.below(400) as usize;

    LogRecordProto {
        timestamp,
        level: 2,
        process: Some("process".to_string()),
        message: random.text(len),
        ctx: vec![LogRecordCtxProto {
            key: "Application".to_string(),
            value: "billing".to_string(),
        }],
    }
}

fn sorted(mut records: Vec<LogRecordProto>) -> Vec<LogRecordProto> {
    records.sort_by(|a, b| (a.timestamp, &a.message).cmp(&(b.timestamp, &b.message)));
    records
}

/// The records by second, and the first entry of each second, as the chain has them
fn check_chain(file: &HourFile, expected: &BTreeMap<usize, Vec<LogRecordProto>>) {
    let entries = file.walk();
    let hour_start = file.hour_start();

    let seconds: Vec<usize> = entries
        .iter()
        .map(|(_, _, time)| second_of(hour_start, *time).unwrap())
        .collect();
    assert!(seconds.windows(2).all(|pair| pair[0] <= pair[1]));

    for second in 0..SECONDS_PER_HOUR {
        let first = entries
            .iter()
            .zip(&seconds)
            .find(|(_, entry_second)| **entry_second == second)
            .map(|((frame, offset, _), _)| Cell {
                frame: *frame,
                offset: *offset as u32,
            });

        assert_eq!(
            first.unwrap_or(Cell::EMPTY),
            file.cells().get(second),
            "second {}",
            second
        );

        assert_eq!(
            file.cells().count(second) as usize,
            expected.get(&second).map_or(0, |records| records.len()),
            "records of second {}",
            second
        );
    }

    let mut by_second: BTreeMap<usize, Vec<LogRecordProto>> = BTreeMap::new();
    for record in file.read_all() {
        by_second
            .entry(second_of(hour_start, record.timestamp).unwrap())
            .or_default()
            .push(record);
    }

    let by_second: BTreeMap<usize, Vec<LogRecordProto>> = by_second
        .into_iter()
        .map(|(second, records)| (second, sorted(records)))
        .collect();

    assert_eq!(&by_second, expected);
}

#[tokio::test]
async fn seconds_written_in_any_order_are_read_in_time_order() {
    let dir = test_dir("any-order");
    let hour = DateHourKey::from(2026101012);
    let hour_start = hour.hour_start().unix_microseconds;

    let mut random = Random(0x2545_F491_4F6C_DD1D);
    let mut file = HourFile::open(&dir, hour, &indexed_keys(), true).unwrap();
    let mut expected: BTreeMap<usize, Vec<LogRecordProto>> = BTreeMap::new();

    // Seconds come in order, now and then a late one - into a second written before or
    // between two written ones - and the late ones are merged into the last block
    for step in 0..1500 {
        let second = if step % 3 == 0 {
            random.below(600) as usize
        } else {
            (step / 3 + 100).min(SECONDS_PER_HOUR - 1)
        };

        let count = 1 + random.below(40) as usize;
        let records: Vec<LogRecordProto> = (0..count)
            .map(|_| {
                let micros = random.below(1_000_000) as i64;
                record(&mut random, hour_start + second as i64 * MICROS_PER_SECOND + micros)
            })
            .collect();

        expected.entry(second).or_default().extend(records.clone());
        file.write_second(second, sorted(records)).unwrap();

        if step % 100 == 0 {
            file.flush(false).unwrap();
        }
    }

    file.flush(true).unwrap();

    // Every record has its application counted in its second
    for (second, records) in &expected {
        assert_eq!(
            file.whole_key_index().count("application", "billing", *second),
            Some(records.len() as u32)
        );
    }

    let expected: BTreeMap<usize, Vec<LogRecordProto>> = expected
        .into_iter()
        .map(|(second, records)| (second, sorted(records)))
        .collect();

    check_chain(&file, &expected);

    // Read back from the files
    let reopened = HourFile::open(&dir, hour, &indexed_keys(), true).unwrap();
    assert_eq!(reopened.cells(), file.cells());
    assert_eq!(reopened.whole_key_index(), file.whole_key_index());

    // An hour which is read keeps only the page of its key index
    let read_only = HourFile::open(&dir, hour, &indexed_keys(), false).unwrap();
    assert!(matches!(read_only.key_index(), HourKeyIndex::Directory { .. }));
    let billing = SearchFilter::new(
        None,
        Some(BTreeMap::from([("application".to_string(), "billing".to_string())])),
        None,
    );
    let counts = read_only.key_index().counts_of(&billing).await;
    for (second, records) in &expected {
        assert_eq!(counts.estimate(u32::MAX, *second), records.len() as u32);
    }

    // The key index is shorter than one read: the read of its page brought in its seconds,
    // and a filter reads nothing more - from a file which is not there, it would fail
    let directory = KeyIndexDirectory::read_local(&dir.join(key_index_file_name(hour))).unwrap();
    let nothing_more = HourKeyIndex::Directory {
        directory: Arc::new(directory),
        source: DataSource::Local(dir.join("not-there")),
    };
    let counts = nothing_more.counts_of(&billing).await;
    for (second, records) in &expected {
        assert_eq!(counts.estimate(u32::MAX, *second), records.len() as u32);
    }
    check_chain(&reopened, &expected);

    // An index whose flush did not complete is rebuilt from the data file
    let index = std::fs::OpenOptions::new()
        .write(true)
        .open(dir.join(index_file_name(hour)))
        .unwrap();
    index.write_all_at(&[INDEX_DIRTY], INDEX_STATE_OFFSET).unwrap();
    drop(index);

    let rebuilt = HourFile::open(&dir, hour, &indexed_keys(), true).unwrap();
    assert_eq!(rebuilt.cells(), file.cells());

    // A key index which does not count what the index does is rebuilt from the data file
    std::fs::remove_file(dir.join(key_index_file_name(hour))).unwrap();
    let rebuilt = HourFile::open(&dir, hour, &indexed_keys(), false).unwrap();
    assert_eq!(rebuilt.whole_key_index(), file.whole_key_index());

    // The copy which is uploaded: the same entries, frames one after another
    let compact_dir = test_dir("any-order-compact");
    file.compact_to(
        &compact_dir.join(data_file_name(hour)),
        &compact_dir.join(index_file_name(hour)),
        &compact_dir.join(key_index_file_name(hour)),
    )
    .unwrap();

    let compact = HourFile::open(&compact_dir, hour, &indexed_keys(), false).unwrap();
    check_chain(&compact, &expected);
    assert_eq!(compact.whole_key_index(), file.whole_key_index());

    let frames: Vec<u32> = compact.walk().iter().map(|(frame, _, _)| *frame).collect();
    assert!(frames.windows(2).all(|pair| pair[1] == pair[0] || pair[1] == pair[0] + FRAME_LEN as u32));
    assert!(
        std::fs::metadata(compact_dir.join(data_file_name(hour))).unwrap().len()
            <= std::fs::metadata(dir.join(data_file_name(hour))).unwrap().len()
    );

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&compact_dir).unwrap();
}

#[test]
fn a_second_waits_in_memory_before_it_is_ready() {
    let now = Instant::now();
    let now_micros = DateTimeAsMicroseconds::now().unix_microseconds;
    let mut pending = PendingLogs::default();

    pending.add(vec![item(LogLevel::Error, now_micros, "billing", "one")], now, now_micros);
    pending.add(vec![item(LogLevel::Debug, now_micros, "billing", "two")], now, now_micros);

    assert!(pending.take_ready(now + Duration::from_secs(1), false).is_empty());
    assert_eq!(pending.len(), 2);

    let ready = pending.take_ready(now + PENDING_DELAY, false);
    let streams: Vec<LogsStream> = ready.iter().map(|bucket| bucket.stream).collect();
    assert_eq!(streams, [LogsStream::Main, LogsStream::DebugInfo]);
    assert_eq!(pending.len(), 0);
}

fn item(level: LogLevel, timestamp: i64, application: &str, message: &str) -> Arc<LogItem> {
    Arc::new(LogItem {
        id: String::new(),
        level,
        process: Some("Process".to_string()),
        message: message.to_string(),
        timestamp: DateTimeAsMicroseconds::new(timestamp),
        ctx: BTreeMap::from([("Application".to_string(), application.to_string())]),
    })
}

async fn write_pending(storage: &Arc<LogsStorage>) {
    let ready = storage.take_ready(true);
    let storage = storage.clone();

    let messages = tokio::task::spawn_blocking(move || storage.write(ready, 24))
        .await
        .unwrap();

    assert!(messages.is_empty(), "{:?}", messages);
}

#[tokio::test]
async fn records_are_found_up_and_down() {
    use SearchDirection::{Down, Up};

    let dir = test_dir("search");
    let storage = Arc::new(LogsStorage::new(&dir, None, Vec::new()));

    let now = DateTimeAsMicroseconds::now().unix_microseconds;
    let start = now - 30 * 60 * MICROS_PER_SECOND;
    let levels = [LogLevel::Error, LogLevel::Debug, LogLevel::Warning, LogLevel::Info];

    let mut items = Vec::new();
    for no in 0..3000i64 {
        let timestamp = start + no * 500_000 + no % 7;
        let application = if no % 3 == 0 { "billing" } else { "trading" };
        let message = if no % 10 == 0 {
            format!("Connection timeout {}", no)
        } else {
            format!("Message {}", no)
        };
        items.push(item(levels[no as usize % 4].clone(), timestamp, application, &message));
    }

    // An application which logs a few times an hour
    for no in [100i64, 1000, 2000, 2900, 2950] {
        let timestamp = start + no * 500_000 + 250_000;
        items.push(item(LogLevel::Warning, timestamp, "Rare", "Rare one"));
    }

    // Two thirds now, the rest late: merged into the seconds already written
    let (late, first): (Vec<_>, Vec<_>) = items.iter().cloned().partition(|item| {
        item.timestamp.unix_microseconds % 3 == 0
    });
    storage.add_pending(first);
    write_pending(&storage).await;
    storage.add_pending(late);
    write_pending(&storage).await;

    let all_filter = SearchFilter::new(None, None, None);
    let (from, to) = (now - 60 * 60 * MICROS_PER_SECOND, now);

    let mut ascending: Vec<i64> = items.iter().map(|item| item.timestamp.unix_microseconds).collect();
    ascending.sort_unstable();
    let descending: Vec<i64> = ascending.iter().rev().copied().collect();

    let times = |found: Vec<LogItemDto>| -> Vec<i64> {
        found.iter().map(|item| item.moment.unix_microseconds).collect()
    };

    assert_eq!(times(storage.search(from, to, &all_filter, 10_000, Down).await), descending);
    assert_eq!(times(storage.search(from, to, &all_filter, 10_000, Up).await), ascending);

    // The limit hit first
    assert_eq!(times(storage.search(from, to, &all_filter, 10, Down).await), descending[..10]);
    assert_eq!(times(storage.search(from, to, &all_filter, 10, Up).await), ascending[..10]);

    // From the middle of the range, the limit hit long before its other end
    let middle = ascending.len() / 2;
    let found = storage.search(from, ascending[middle], &all_filter, 25, Down).await;
    let expected: Vec<i64> = ascending[middle - 24..=middle].iter().rev().copied().collect();
    assert_eq!(times(found), expected);
    let found = storage.search(ascending[middle], to, &all_filter, 25, Up).await;
    assert_eq!(times(found), ascending[middle..middle + 25]);

    // The end of the range hit first
    let fifth_from_end = ascending[ascending.len() - 5];
    assert_eq!(storage.search(fifth_from_end, to, &all_filter, 100, Down).await.len(), 5);
    assert_eq!(storage.search(from, ascending[4], &all_filter, 100, Up).await.len(), 5);

    // The key index finds the seconds of a rare application
    let rare = SearchFilter::new(
        None,
        Some(BTreeMap::from([("Application".to_string(), "RARE".to_string())])),
        None,
    );
    let rare_times: Vec<i64> = items
        .iter()
        .filter(|item| item.ctx["Application"] == "Rare")
        .map(|item| item.timestamp.unix_microseconds)
        .collect();
    let found = storage.search(from, to, &rare, 3, Down).await;
    let latest_rare: Vec<i64> = rare_times.iter().rev().take(3).copied().collect();
    assert_eq!(times(found), latest_rare);
    assert_eq!(times(storage.search(from, to, &rare, 3, Up).await), rare_times[..3]);
    assert_eq!(storage.search(from, to, &rare, 100, Down).await.len(), 5);

    // A filter which leaves a quarter of the records, and a limit
    let errors = SearchFilter::new(Some(vec![LogLevelDto::Error]), None, None);
    let error_times: Vec<i64> = {
        let mut times: Vec<i64> = items
            .iter()
            .filter(|item| matches!(item.level, LogLevel::Error))
            .map(|item| item.timestamp.unix_microseconds)
            .collect();
        times.sort_unstable();
        times
    };
    let found = storage.search(from, to, &errors, 10, Up).await;
    assert_eq!(times(found), error_times[..10]);
    let found = storage.search(from, to, &errors, 10, Down).await;
    let latest_errors: Vec<i64> = error_times.iter().rev().take(10).copied().collect();
    assert_eq!(times(found), latest_errors);

    for direction in [Down, Up] {
        let found = storage.search(from, to, &errors, 10_000, direction).await;
        assert_eq!(found.len(), 750);
        assert!(found.iter().all(|item| item.level == LogLevelDto::Error));

        let billing = SearchFilter::new(
            None,
            Some(BTreeMap::from([("application".to_string(), "BILLING".to_string())])),
            None,
        );
        assert_eq!(storage.search(from, to, &billing, 10_000, direction).await.len(), 1000);

        let timeouts = SearchFilter::new(None, None, Some("TIMEOUT"));
        assert_eq!(storage.search(from, to, &timeouts, 10_000, direction).await.len(), 300);
    }

    // A record waiting in memory is found as well
    storage.add_pending(vec![item(LogLevel::FatalError, now - 1, "billing", "Just now")]);
    let found = storage.search(from, to, &all_filter, 1, Down).await;
    assert_eq!(found[0].message, "Just now");
    write_pending(&storage).await;

    // And everything is there once the files are opened again
    let reopened = LogsStorage::new(&dir, None, Vec::new());
    let found = reopened.search(from, to, &all_filter, 10_000, Up).await;
    assert_eq!(found.len(), items.len() + 1);

    let mut counts: HashMap<String, usize> = HashMap::new();
    for item in &found {
        *counts.entry(item.context["Application"].clone()).or_default() += 1;
    }
    assert_eq!(counts["billing"], 1001);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn an_hour_which_is_read_finds_its_seconds_in_the_page_of_its_key_index() {
    use SearchDirection::{Down, Up};

    let dir = test_dir("read-hour");
    let storage = Arc::new(LogsStorage::new(&dir, None, Vec::new()));

    // Two hours ago: neither the current hour nor the one before
    let now = DateTimeAsMicroseconds::now().unix_microseconds;
    let hour = DateHourKey::new(DateTimeAsMicroseconds::new(now - 2 * 3600 * MICROS_PER_SECOND));
    let hour_start = hour.hour_start().unix_microseconds;

    let mut items = Vec::new();
    for no in 0..2000i64 {
        let application = if no % 2 == 0 { "billing" } else { "trading" };
        items.push(item(LogLevel::Error, hour_start + no * 1_800_000, application, "Message"));
    }
    let rare_times: Vec<i64> = [100i64, 1700, 3500]
        .iter()
        .map(|second| hour_start + second * MICROS_PER_SECOND + 1)
        .collect();
    for timestamp in &rare_times {
        items.push(item(LogLevel::Error, *timestamp, "Rare", "Rare one"));
    }

    storage.add_pending(items);
    write_pending(&storage).await;

    let reopened = LogsStorage::new(&dir, None, Vec::new());
    let (from, to) = (hour_start, hour_start + 3600 * MICROS_PER_SECOND - 1);

    let rare = SearchFilter::new(
        None,
        Some(BTreeMap::from([("application".to_string(), "rare".to_string())])),
        None,
    );
    let times = |found: Vec<LogItemDto>| -> Vec<i64> {
        found.iter().map(|item| item.moment.unix_microseconds).collect()
    };

    assert_eq!(times(reopened.search(from, to, &rare, 2, Down).await), [rare_times[2], rare_times[1]]);
    assert_eq!(times(reopened.search(from, to, &rare, 2, Up).await), rare_times[..2]);

    let billing = SearchFilter::new(
        None,
        Some(BTreeMap::from([("application".to_string(), "Billing".to_string())])),
        None,
    );
    assert_eq!(reopened.search(from, to, &billing, 10_000, Down).await.len(), 1000);

    std::fs::remove_dir_all(&dir).unwrap();
}
