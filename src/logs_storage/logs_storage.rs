use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Instant,
};

use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::{
    app::LogItem,
    product_scopes::{is_valid_scope_name, ProductScopes},
    repo::{dto::LogItemDto, DateHourKey},
};

use super::{
    data_file_name, decompress_records, index_file_name, scan_seconds, second_of, DataSource,
    key_index_file_name, DataFiles, Entry, FilterCounts, HourCells, HourFile, HourKeyIndex,
    LogsStream, PendingLogs, ReadyBucket, S3Archive, SearchFilter, DATA_FILE_LEN, FRAME_LEN,
    MAX_DATA_FILES, MICROS_PER_SECOND,
};

/// Hours stay on the local disk this long after they end. Then the Warning, Error and
/// FatalError hours are uploaded to S3 when it is set up; the other hours are deleted.
pub const LOCAL_HOURS: i64 = 2;

const HOUR_MICROS: i64 = 3600 * MICROS_PER_SECOND;

/// An hour not read or written for this long is let go - except the current hour and the
/// one before it, whose indexes stay in memory
const IDLE_HOUR_MICROS: i64 = 10 * 60 * MICROS_PER_SECOND;

/// What one read of a local data file brings in: a window is not read for less
const LOCAL_READ_LEN: u64 = 64 * 1024;

/// What a request to S3 is worth: one costs as much as reading this much more
const S3_READ_LEN: u64 = 1024 * 1024;

/// A search asking for no limit gets this many records at most
const DEFAULT_LIMIT: usize = 1000;

/// An hour of a stream of a product scope
type HourKey = (LogsStream, String, DateHourKey);

pub struct HourHandle {
    pub file: Arc<tokio::sync::RwLock<HourFile>>,
    last_access: AtomicI64,
}

impl HourHandle {
    fn new(file: HourFile, now: i64) -> Self {
        Self {
            file: Arc::new(tokio::sync::RwLock::new(file)),
            last_access: AtomicI64::new(now),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// From `from` up: the oldest first, until `to` or `limit` records
    Up,
    /// From `to` down: the latest first, until `from` or `limit` records
    Down,
}

/// A data file of an hour a search reads, with what its indexes say of the filter
struct HourSource {
    data: DataFiles,
    cells: Arc<HourCells>,
    counts: FilterCounts,
}

impl HourSource {
    /// At most how many records of the second match
    fn estimate(&self, second: usize) -> u64 {
        self.counts.estimate(self.cells.count(second), second) as u64
    }

    /// The seconds of `from_second ..= to_second` to read, as runs read in one pass each:
    /// the seconds which may hold a match, and between two of them the seconds which do
    /// not, while they hold fewer records than one read brings in
    fn runs(
        &self,
        from_second: usize,
        to_second: usize,
        records_per_read: u64,
    ) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        let mut current: Option<(usize, usize)> = None;
        let mut gap = 0;

        for second in from_second..=to_second {
            let records = self.cells.count(second) as u64;

            if records == 0 {
                continue;
            }

            if self.estimate(second) == 0 {
                gap += records;
                continue;
            }

            current = match current {
                Some((start, _)) if gap < records_per_read => Some((start, second)),
                Some(run) => {
                    result.push(run);
                    Some((second, second))
                }
                None => Some((second, second)),
            };

            gap = 0;
        }

        result.extend(current);
        result
    }
}

struct HourToRead {
    _files: Vec<tokio::sync::OwnedRwLockReadGuard<HourFile>>,
    sources: Vec<HourSource>,
}

impl HourToRead {
    /// The records of the second in all the files read
    fn count(&self, second: usize) -> u64 {
        self.sources
            .iter()
            .map(|source| source.cells.count(second) as u64)
            .sum()
    }

    /// At most how many records of the second match, in all the files read
    fn estimate(&self, second: usize) -> u64 {
        self.sources.iter().map(|source| source.estimate(second)).sum()
    }

    /// About how many records one read brings in: the records of the hour by the bytes of
    /// its data files, the index alone telling both
    fn records_per_read(&self) -> u64 {
        let read_len = if self
            .sources
            .iter()
            .any(|source| matches!(source.data, DataFiles::S3 { .. }))
        {
            S3_READ_LEN
        } else {
            LOCAL_READ_LEN
        };

        let mut records = 0;
        let mut bytes = 0;

        for source in &self.sources {
            records += source.cells.total();

            if let Some((_, last)) = source.cells.last() {
                bytes += last.frame.file as u64 * DATA_FILE_LEN
                    + last.frame.offset as u64
                    + FRAME_LEN as u64;
            }
        }

        if bytes == 0 {
            return 0;
        }

        read_len * records / bytes
    }
}

/// How many records which may match the next pass is to take in: what is missing, scaled
/// by the share of the estimated ones which matched in the pass before
fn next_wanted(missing: u64, estimated: u64, matched: u64) -> u64 {
    if matched == 0 {
        estimated.max(1) * 2
    } else {
        (missing * estimated).div_ceil(matched)
    }
}

pub struct StorageStats {
    pub pending_records: usize,
    pub open_hours: Vec<String>,
    pub cached_frames: usize,
}

/// The logs: by product scope and hour, `{LogsDbPath}/logs/{scope}` for Warning, Error and
/// FatalError, `{LogsDbPath}/debug-info/{scope}` for Debug and Info. What came in the last
/// seconds waits in memory before it is written.
pub struct LogsStorage {
    root: PathBuf,
    hours: Mutex<BTreeMap<HourKey, Arc<HourHandle>>>,
    pending: Mutex<PendingLogs>,
    s3: Option<Arc<S3Archive>>,
    /// The context keys a new hour counts the values of, besides the level and the process
    indexed_keys: Vec<String>,
    product_scopes: RwLock<Arc<ProductScopes>>,
}

impl LogsStorage {
    pub fn new(
        root: impl Into<PathBuf>,
        s3: Option<S3Archive>,
        product_scopes: ProductScopes,
    ) -> Self {
        let root = root.into();

        for stream in LogsStream::ALL {
            if let Err(err) = std::fs::create_dir_all(root.join(stream.dir_name())) {
                println!("Can not create {}: {}", root.join(stream.dir_name()).display(), err);
            }
        }

        Self {
            root,
            hours: Mutex::new(BTreeMap::new()),
            pending: Mutex::new(PendingLogs::default()),
            s3: s3.map(Arc::new),
            indexed_keys: ["Application", "Version"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            product_scopes: RwLock::new(Arc::new(product_scopes)),
        }
    }

    pub fn has_s3(&self) -> bool {
        self.s3.is_some()
    }

    pub fn product_scopes(&self) -> Arc<ProductScopes> {
        self.product_scopes.read().unwrap().clone()
    }

    /// The records which come from now on go to these scopes; the written ones stay where
    /// they are
    pub fn set_product_scopes(&self, product_scopes: ProductScopes) {
        *self.product_scopes.write().unwrap() = Arc::new(product_scopes);
    }

    fn dir(&self, stream: LogsStream, scope: &str) -> PathBuf {
        self.root.join(stream.dir_name()).join(scope)
    }

    /// The records go to `scope` - the one the writer names - or, when it names none, by
    /// the product scopes. Returns how many records were dropped to keep the buffer bounded.
    pub fn add_pending(&self, items: Vec<Arc<LogItem>>, scope: Option<&str>) -> usize {
        let scopes = self.product_scopes();

        self.pending.lock().unwrap().add(
            items,
            Instant::now(),
            DateTimeAsMicroseconds::now().unix_microseconds,
            |item| scopes.record_scope(scope, item).to_string(),
        )
    }

    /// The scopes there are folders of: the ones the records went to, set or not
    pub fn scopes_with_records(&self) -> BTreeSet<String> {
        self.local_scopes(LogsStream::Main)
            .into_iter()
            .chain(self.local_scopes(LogsStream::DebugInfo))
            .collect()
    }

    /// The seconds which have waited long enough - or all of them
    pub fn take_ready(&self, all: bool) -> Vec<ReadyBucket> {
        self.pending.lock().unwrap().take_ready(Instant::now(), all)
    }

    /// Writes the seconds: each hour under its write lock, and flushed before it is let
    /// go. Returns what went wrong.
    pub fn write(&self, buckets: Vec<ReadyBucket>) -> Vec<String> {
        let now = DateTimeAsMicroseconds::now().unix_microseconds;
        let mut messages = Vec::new();

        let mut by_hour: BTreeMap<HourKey, Vec<ReadyBucket>> = BTreeMap::new();

        for bucket in buckets {
            let hour = DateHourKey::new(DateTimeAsMicroseconds::new(
                bucket.second * MICROS_PER_SECOND,
            ));
            by_hour
                .entry((bucket.stream, bucket.scope.clone(), hour))
                .or_default()
                .push(bucket);
        }

        for ((stream, scope, hour), buckets) in by_hour {
            let name = hour_name(stream, &scope, hour);

            if local_time_is_over(hour, now) {
                let records: usize = buckets.iter().map(|bucket| bucket.records.len()).sum();
                messages.push(format!(
                    "{} records of {} are dropped: the hour is not kept on the local disk any more",
                    records, name
                ));
                continue;
            }

            let handle = match self.get_hour(stream, &scope, hour, true, now) {
                Ok(handle) => handle.unwrap(),
                Err(err) => {
                    messages.push(format!("{} does not open: {}", name, err));
                    continue;
                }
            };

            let mut file = handle.file.blocking_write();

            if file.is_sealed() {
                let records: usize = buckets.iter().map(|bucket| bucket.records.len()).sum();
                messages.push(format!(
                    "{} records of {} are dropped: the hour is being uploaded",
                    records, name
                ));
                continue;
            }

            let hour_start = file.hour_start();

            for bucket in buckets {
                let Some(second) = second_of(hour_start, bucket.second * MICROS_PER_SECOND) else {
                    continue;
                };

                if let Err(err) = file.write_second(second, bucket.into_protos()) {
                    messages.push(format!("Second {} of {} is not written: {}", second, name, err));
                }
            }

            if let Err(err) = file.flush(false) {
                messages.push(format!("{} is not flushed: {}", name, err));
            }
        }

        messages
    }

    /// The hour, opened when it is not yet. Without `create` an hour with no data file is
    /// `None`.
    fn get_hour(
        &self,
        stream: LogsStream,
        scope: &str,
        hour: DateHourKey,
        create: bool,
        now: i64,
    ) -> std::io::Result<Option<Arc<HourHandle>>> {
        let key = (stream, scope.to_string(), hour);
        let mut hours = self.hours.lock().unwrap();

        if let Some(handle) = hours.get(&key) {
            handle.last_access.store(now, Ordering::Relaxed);
            return Ok(Some(handle.clone()));
        }

        let dir = self.dir(stream, scope);

        if !create && !dir.join(data_file_name(hour, 0)).exists() {
            return Ok(None);
        }

        // The current hour and the one before are written into: their key index is kept whole
        let previous_hour = DateHourKey::new(DateTimeAsMicroseconds::new(now - HOUR_MICROS));
        let full_key_index = create || hour >= previous_hour;

        let handle = Arc::new(HourHandle::new(
            HourFile::open(&dir, hour, &self.indexed_keys, full_key_index)?,
            now,
        ));
        hours.insert(key, handle.clone());

        Ok(Some(handle))
    }

    /// The records of the product scope in `from ..= to` which match, at most `limit` of
    /// them: from `from` up, the oldest first, or from `to` down, the latest first. A scope
    /// which can not name a folder has none.
    pub async fn search(
        &self,
        scope: &str,
        from: i64,
        to: i64,
        filter: &SearchFilter,
        limit: usize,
        direction: SearchDirection,
    ) -> Vec<LogItemDto> {
        if !is_valid_scope_name(scope) {
            return Vec::new();
        }

        let limit = if limit == 0 { DEFAULT_LIMIT } else { limit };

        let mut result = match direction {
            SearchDirection::Up => self.search_up(scope, from, to, filter, limit).await,
            SearchDirection::Down => self.search_down(scope, from, to, filter, limit).await,
        };

        // What waits in memory falls into its place among the records of the files
        result.extend(self.pending.lock().unwrap().search(scope, from, to, filter));

        match direction {
            SearchDirection::Up => result.sort_by_key(|item| item.moment.unix_microseconds),
            SearchDirection::Down => {
                result.sort_by(|a, b| b.moment.unix_microseconds.cmp(&a.moment.unix_microseconds))
            }
        }

        result.truncate(limit);
        result
    }

    /// The hours one after another. In each, the top second of a pass is found in the
    /// indexes - up until the records which may match add up to what is missing, and the
    /// records to one read at least - and the seconds which may match are read up to it,
    /// until `limit` records are found or the range is over. The second in which they are
    /// found is read to its end: its blocks are not in time order between themselves.
    async fn search_up(
        &self,
        scope: &str,
        from: i64,
        to: i64,
        filter: &SearchFilter,
        limit: usize,
    ) -> Vec<LogItemDto> {
        let now = DateTimeAsMicroseconds::now().unix_microseconds;
        let mut result = Vec::new();

        for hour in hours_of(from, to).keys().copied() {
            if result.len() >= limit {
                break;
            }

            let hour_to_read = self.open_for_reading(scope, hour, filter, now).await;

            if hour_to_read.sources.is_empty() {
                continue;
            }

            let hour_start = hour.hour_start().unix_microseconds;
            let (first_second, last_second) = seconds_of(hour_start, from, to);
            let records_per_read = hour_to_read.records_per_read();

            let mut window_start = first_second;
            let mut wanted = (limit - result.len()) as u64;

            loop {
                let mut window_end = window_start;
                let mut estimated = hour_to_read.estimate(window_start);
                let mut counted = hour_to_read.count(window_start);

                while (estimated < wanted || counted < records_per_read) && window_end < last_second
                {
                    window_end += 1;
                    estimated += hour_to_read.estimate(window_end);
                    counted += hour_to_read.count(window_end);
                }

                let missing = limit - result.len();
                let mut in_window = Vec::new();

                for source in &hour_to_read.sources {
                    let mut found = Vec::new();
                    let mut stop_at = None;

                    for (run_start, run_end) in
                        source.runs(window_start, window_end, records_per_read)
                    {
                        if stop_at.is_some() {
                            break;
                        }

                        scan_hour(hour, &source.data, &source.cells, run_start, run_end, |entry| {
                            if stop_at.map_or(false, |stop_at| entry.time >= stop_at) {
                                return false;
                            }

                            collect_matches(entry, hour, from, to, filter, &mut found);

                            if stop_at.is_none() && found.len() >= missing {
                                let second = entry.time.div_euclid(MICROS_PER_SECOND);
                                stop_at = Some((second + 1) * MICROS_PER_SECOND);
                            }

                            true
                        })
                        .await;
                    }

                    in_window.extend(found);
                }

                let matched = in_window.len() as u64;
                in_window.sort_by_key(|item| item.moment.unix_microseconds);
                result.extend(in_window);

                if result.len() >= limit || window_end == last_second {
                    break;
                }

                window_start = window_end + 1;
                wanted = next_wanted((limit - result.len()) as u64, estimated, matched);
            }
        }

        result
    }

    /// The bottom second of a pass is found in the indexes, which are in memory: down from
    /// `to` until the records which may match add up to `limit` - and the records to one
    /// read at least - and the seconds which may match are read up from it to `to`. When
    /// fewer records match than the indexes allow - a phrase, more than one key - the next
    /// bottom second is found the same way for what is missing, scaled by the share which
    /// matched.
    async fn search_down(
        &self,
        scope: &str,
        from: i64,
        to: i64,
        filter: &SearchFilter,
        limit: usize,
    ) -> Vec<LogItemDto> {
        let now = DateTimeAsMicroseconds::now().unix_microseconds;
        let mut result = Vec::new();

        for hour in hours_of(from, to).keys().rev().copied() {
            if result.len() >= limit {
                break;
            }

            let hour_to_read = self.open_for_reading(scope, hour, filter, now).await;

            if hour_to_read.sources.is_empty() {
                continue;
            }

            let hour_start = hour.hour_start().unix_microseconds;
            let (first_second, last_second) = seconds_of(hour_start, from, to);
            let records_per_read = hour_to_read.records_per_read();

            let mut window_end = last_second;
            let mut wanted = (limit - result.len()) as u64;

            loop {
                let mut window_start = window_end;
                let mut estimated = hour_to_read.estimate(window_end);
                let mut counted = hour_to_read.count(window_end);

                while (estimated < wanted || counted < records_per_read)
                    && window_start > first_second
                {
                    window_start -= 1;
                    estimated += hour_to_read.estimate(window_start);
                    counted += hour_to_read.count(window_start);
                }

                let mut window = Vec::new();

                for source in &hour_to_read.sources {
                    for (run_start, run_end) in
                        source.runs(window_start, window_end, records_per_read)
                    {
                        scan_hour(hour, &source.data, &source.cells, run_start, run_end, |entry| {
                            collect_matches(entry, hour, from, to, filter, &mut window);
                            true
                        })
                        .await;
                    }
                }

                let matched = window.len() as u64;
                window.sort_by(|a, b| b.moment.unix_microseconds.cmp(&a.moment.unix_microseconds));
                result.extend(window);

                if result.len() >= limit || window_start == first_second {
                    break;
                }

                window_end = window_start - 1;
                wanted = next_wanted((limit - result.len()) as u64, estimated, matched);
            }
        }

        result
    }

    /// What a search reads of an hour: the local files, under their read locks so the writes
    /// into the hour wait, or the hour in S3
    async fn open_for_reading(
        &self,
        scope: &str,
        hour: DateHourKey,
        filter: &SearchFilter,
        now: i64,
    ) -> HourToRead {
        let mut result = HourToRead {
            _files: Vec::new(),
            sources: Vec::new(),
        };

        for stream in LogsStream::ALL {
            if !filter.wants(stream) {
                continue;
            }

            match self.get_hour(stream, scope, hour, false, now) {
                Ok(Some(handle)) => {
                    let file = handle.file.clone().read_owned().await;

                    if file.has_entries() {
                        result.sources.push(HourSource {
                            data: file.data_files(),
                            cells: Arc::new(file.cells().clone()),
                            counts: file.key_index().counts_of(filter).await,
                        });
                    }

                    result._files.push(file);
                }
                Ok(None) => {
                    if let (LogsStream::Main, Some(s3)) = (stream, &self.s3) {
                        match S3Archive::get_indexes(s3, scope, hour).await {
                            Ok(Some((cells, directory))) => {
                                let key_index = HourKeyIndex::Directory {
                                    directory,
                                    source: DataSource::S3 {
                                        archive: s3.clone(),
                                        file_name: scoped_name(scope, key_index_file_name(hour)),
                                    },
                                };

                                result.sources.push(HourSource {
                                    data: DataFiles::S3 {
                                        archive: s3.clone(),
                                        scope: scope.to_string(),
                                        hour,
                                    },
                                    cells,
                                    counts: key_index.counts_of(filter).await,
                                });
                            }
                            Ok(None) => {}
                            Err(err) => println!("{}", err),
                        }
                    }
                }
                Err(err) => println!("{} does not open: {}", hour_name(stream, scope, hour), err),
            }
        }

        result
    }

    /// Deletes the local hours whose time is over and lets go of the idle ones. Returns
    /// what was done.
    pub fn gc(&self, now: DateTimeAsMicroseconds, hours_to_gc: u64) -> Vec<String> {
        let now = now.unix_microseconds;
        let mut messages = Vec::new();

        for stream in LogsStream::ALL {
            for scope in self.local_scopes(stream) {
                for hour in self.local_hours(stream, &scope) {
                    // With S3 the Main hours are uploaded long before - unless the uploads
                    // fail as long, or they are not kept as long as it takes to upload them
                    let expired = match stream {
                        LogsStream::Main if self.s3.is_some() => {
                            local_time_is_over(hour, now) && main_is_expired(hour, now, hours_to_gc)
                        }
                        LogsStream::Main | LogsStream::DebugInfo => local_time_is_over(hour, now),
                    };

                    if !expired {
                        continue;
                    }

                    let name = hour_name(stream, &scope, hour);

                    match self.delete_local(stream, &scope, hour) {
                        Ok(()) => messages.push(format!("GC: {} is deleted", name)),
                        Err(err) => messages.push(format!("GC: {} is not deleted: {}", name, err)),
                    }
                }
            }
        }

        messages.extend(self.persist_key_indexes(false));
        self.let_go_idle_hours(now);

        // Copies an upload left behind when the process stopped during it
        for path in self.upload_copies() {
            let hour = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.get(..10))
                .and_then(DateHourKey::parse_from_str);

            if hour.map_or(true, |hour| main_is_expired(hour, now, hours_to_gc))
                && std::fs::remove_file(&path).is_ok()
            {
                messages.push(format!("GC: {} is deleted", path.display()));
            }
        }

        messages
    }

    /// The product scopes the stream has folders of
    fn local_scopes(&self, stream: LogsStream) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.root.join(stream.dir_name())) else {
            return Vec::new();
        };

        entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
            .filter(|name| is_valid_scope_name(name))
            .collect()
    }

    /// The hours the stream of the scope has data files of
    fn local_hours(&self, stream: LogsStream, scope: &str) -> BTreeSet<DateHourKey> {
        let mut result = BTreeSet::new();

        let Ok(entries) = std::fs::read_dir(self.dir(stream, scope)) else {
            return result;
        };

        for entry in entries.flatten() {
            let hour = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".data00"))
                .and_then(DateHourKey::parse_from_str);

            if let Some(hour) = hour {
                result.insert(hour);
            }
        }

        result
    }

    fn upload_copies(&self) -> Vec<PathBuf> {
        self.local_scopes(LogsStream::Main)
            .into_iter()
            .filter_map(|scope| std::fs::read_dir(self.dir(LogsStream::Main, &scope)).ok())
            .flat_map(|entries| entries.flatten().map(|entry| entry.path()))
            .filter(|path| path.extension().map_or(false, |extension| extension == "upload"))
            .collect()
    }

    /// Writes the key indexes of the open hours which are changed - when it is time, or
    /// all of them. Returns what went wrong.
    pub fn persist_key_indexes(&self, force: bool) -> Vec<String> {
        let handles: Vec<(HourKey, Arc<HourHandle>)> = self
            .hours
            .lock()
            .unwrap()
            .iter()
            .map(|(key, handle)| (key.clone(), handle.clone()))
            .collect();

        let mut messages = Vec::new();

        for ((stream, scope, hour), handle) in handles {
            if let Err(err) = handle.file.blocking_write().flush(force) {
                messages.push(format!(
                    "{} is not flushed: {}",
                    hour_name(stream, &scope, hour),
                    err
                ));
            }
        }

        messages
    }

    fn delete_local(&self, stream: LogsStream, scope: &str, hour: DateHourKey) -> std::io::Result<()> {
        let handle = self
            .hours
            .lock()
            .unwrap()
            .remove(&(stream, scope.to_string(), hour));

        // A read or a write which holds the hour ends first
        let _file = handle.as_ref().map(|handle| handle.file.blocking_write());

        let dir = self.dir(stream, scope);

        for file in 0..MAX_DATA_FILES {
            let path = dir.join(data_file_name(hour, file as u8));

            if !path.exists() {
                break;
            }

            std::fs::remove_file(path)?;
        }

        remove_if_exists(&dir.join(index_file_name(hour)))?;
        remove_if_exists(&dir.join(key_index_file_name(hour)))?;

        Ok(())
    }

    fn let_go_idle_hours(&self, now: i64) {
        let current = DateHourKey::new(DateTimeAsMicroseconds::new(now));
        let previous = DateHourKey::new(DateTimeAsMicroseconds::new(now - HOUR_MICROS));

        self.hours.lock().unwrap().retain(|(_, _, hour), handle| {
            *hour == current
                || *hour == previous
                || now - handle.last_access.load(Ordering::Relaxed) < IDLE_HOUR_MICROS
        });
    }

    /// The Main hours of every product scope whose time on the local disk is over and which
    /// are not expired: to be uploaded. With `hours_to_gc` of 3 or less none is - they are
    /// deleted instead.
    pub fn hours_to_upload(
        &self,
        now: DateTimeAsMicroseconds,
        hours_to_gc: u64,
    ) -> Vec<(String, DateHourKey)> {
        let now = now.unix_microseconds;

        self.local_scopes(LogsStream::Main)
            .into_iter()
            .flat_map(|scope| {
                self.local_hours(LogsStream::Main, &scope)
                    .into_iter()
                    .filter(|hour| {
                        local_time_is_over(*hour, now) && !main_is_expired(*hour, now, hours_to_gc)
                    })
                    .map(move |hour| (scope.clone(), hour))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Seals the hour - nothing is written into it any more - copies its chain into files
    /// whose frames follow one another, uploads them and deletes the local files. A failed
    /// upload unseals the hour: the next round tries it again.
    pub async fn upload_hour(self: Arc<Self>, scope: String, hour: DateHourKey) -> Result<(), String> {
        let Some(s3) = self.s3.clone() else {
            return Err("S3 is not set up".to_string());
        };

        let dir = self.dir(LogsStream::Main, &scope);
        let index_copy = dir.join(format!("{}.upload", index_file_name(hour)));
        let key_index_copy = dir.join(format!("{}.upload", key_index_file_name(hour)));

        let storage = self.clone();
        let copies_dir = dir.clone();
        let copies = (index_copy.clone(), key_index_copy.clone());
        let copies_scope = scope.clone();
        let copied = tokio::task::spawn_blocking(move || {
            storage.seal_and_copy(&copies_scope, hour, &copies_dir, &copies.0, &copies.1)
        })
        .await
        .map_err(|err| err.to_string())?;

        let uploaded = match &copied {
            Ok(data_copies) => {
                s3.upload(&scope, hour, data_copies, &index_copy, &key_index_copy)
                    .await
            }
            Err(err) => Err(err.clone()),
        };

        for copy in copied
            .iter()
            .flatten()
            .chain([&index_copy, &key_index_copy])
        {
            let _ = tokio::fs::remove_file(copy).await;
        }

        let storage = self.clone();
        tokio::task::spawn_blocking(move || match uploaded {
            Ok(()) => storage
                .delete_local(LogsStream::Main, &scope, hour)
                .map_err(|err| format!("Uploaded, and not deleted locally: {}", err)),
            Err(err) => {
                storage.set_sealed(LogsStream::Main, &scope, hour, false);
                Err(err)
            }
        })
        .await
        .map_err(|err| err.to_string())?
    }

    /// Returns the copies of the data files, `.data00.upload` ...
    fn seal_and_copy(
        &self,
        scope: &str,
        hour: DateHourKey,
        dir: &Path,
        index: &Path,
        key_index: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        let now = DateTimeAsMicroseconds::now().unix_microseconds;

        let handle = self
            .get_hour(LogsStream::Main, scope, hour, false, now)
            .map_err(|err| err.to_string())?
            .ok_or_else(|| format!("{} has no local files", hour_name(LogsStream::Main, scope, hour)))?;

        let mut file = handle.file.blocking_write();
        file.set_sealed(true);

        file.flush(true).map_err(|err| err.to_string())?;

        let data_copy = |file: u8| dir.join(format!("{}.upload", data_file_name(hour, file)));

        let files = file
            .compact_to(data_copy, index, key_index)
            .map_err(|err| err.to_string())?;

        Ok((0..files).map(data_copy).collect())
    }

    fn set_sealed(&self, stream: LogsStream, scope: &str, hour: DateHourKey, sealed: bool) {
        let handle = self
            .hours
            .lock()
            .unwrap()
            .get(&(stream, scope.to_string(), hour))
            .cloned();

        if let Some(handle) = handle {
            handle.file.blocking_write().set_sealed(sealed);
        }
    }

    /// Deletes the hours of S3 older than `hours_to_gc`, of every product scope
    pub async fn gc_s3(
        &self,
        now: DateTimeAsMicroseconds,
        hours_to_gc: u64,
    ) -> Result<Vec<(String, DateHourKey)>, String> {
        match &self.s3 {
            Some(s3) => {
                s3.delete_up_to(main_cutoff(now.unix_microseconds, hours_to_gc))
                    .await
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn stats(&self) -> StorageStats {
        let pending_records = self.pending.lock().unwrap().len();
        let hours = self.hours.lock().unwrap();

        StorageStats {
            pending_records,
            open_hours: hours
                .keys()
                .map(|(stream, scope, hour)| hour_name(*stream, scope, *hour))
                .collect(),
            cached_frames: hours
                .values()
                .filter_map(|handle| handle.file.try_read().ok().map(|file| file.cached_frames()))
                .sum(),
        }
    }
}

/// `logs/trading/2026101112`
fn hour_name(stream: LogsStream, scope: &str, hour: DateHourKey) -> String {
    format!("{}/{}/{}", stream.dir_name(), scope, hour.get_value())
}

/// The name of a file of an hour of the scope in S3, past the prefix
pub fn scoped_name(scope: &str, file_name: String) -> String {
    format!("{}/{}", scope, file_name)
}

fn hours_of(from: i64, to: i64) -> BTreeMap<DateHourKey, ()> {
    DateHourKey::get_keys_to_request(
        DateTimeAsMicroseconds::new(from),
        DateTimeAsMicroseconds::new(to),
    )
}

/// The first and the last second of the hour within `from ..= to`
fn seconds_of(hour_start: i64, from: i64, to: i64) -> (usize, usize) {
    (
        second_of(hour_start, from.max(hour_start)).unwrap(),
        second_of(hour_start, to.min(hour_start + HOUR_MICROS - 1)).unwrap(),
    )
}

async fn scan_hour(
    hour: DateHourKey,
    source: &DataFiles,
    cells: &HourCells,
    from_second: usize,
    to_second: usize,
    on_entry: impl FnMut(&Entry) -> bool,
) {
    let hour_start = hour.hour_start().unix_microseconds;

    if let Err(err) =
        scan_seconds(source.clone(), cells, hour_start, from_second, to_second, on_entry).await
    {
        println!("{} is not read to its end: {}", hour.get_value(), err);
    }
}

/// The records of the entry within `from ..= to` which match
fn collect_matches(
    entry: &Entry,
    hour: DateHourKey,
    from: i64,
    to: i64,
    filter: &SearchFilter,
    out: &mut Vec<LogItemDto>,
) {
    match decompress_records(entry.payload) {
        Ok(records) => {
            for record in records {
                if record.timestamp >= from && record.timestamp <= to && filter.matches_record(&record) {
                    out.push(record.into_dto());
                }
            }
        }
        Err(err) => println!("A block of {} does not read: {}", hour.get_value(), err),
    }
}

fn local_time_is_over(hour: DateHourKey, now: i64) -> bool {
    hour.hour_start().unix_microseconds + HOUR_MICROS + LOCAL_HOURS * HOUR_MICROS <= now
}

fn main_cutoff(now: i64, hours_to_gc: u64) -> DateHourKey {
    DateHourKey::new(DateTimeAsMicroseconds::new(
        now - hours_to_gc as i64 * HOUR_MICROS,
    ))
}

fn main_is_expired(hour: DateHourKey, now: i64, hours_to_gc: u64) -> bool {
    hour <= main_cutoff(now, hours_to_gc)
}

fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs_storage::{Cell, FrameId, KeyIndex, LogRecordCtxProto, LogRecordProto};

    #[tokio::test]
    async fn seconds_far_apart_are_read_in_passes_of_their_own() {
        let mut cells = HourCells::new();
        for second in 0..1000 {
            cells.set(
                second,
                Cell {
                    frame: FrameId::HEAD,
                    offset: 11,
                },
            );
            cells.set_count(second, 100);
        }

        let mut key_index = KeyIndex::new(&["Application".to_string()]);
        let record = LogRecordProto {
            timestamp: 0,
            level: 2,
            process: None,
            message: String::new(),
            ctx: vec![LogRecordCtxProto {
                key: "Application".to_string(),
                value: "Rare".to_string(),
            }],
        };
        for second in [10, 11, 500] {
            key_index.add_records(second, &[record.clone()]);
        }

        let key_index = HourKeyIndex::Full(Arc::new(key_index));
        let cells = Arc::new(cells);

        let source_of = |counts: FilterCounts| HourSource {
            data: DataFiles::Local {
                dir: PathBuf::new(),
                hour: DateHourKey::from(2026101112),
            },
            cells: cells.clone(),
            counts,
        };

        let rare = SearchFilter::new(
            None,
            Some(BTreeMap::from([("application".to_string(), "rare".to_string())])),
            None,
        );
        let source = source_of(key_index.counts_of(&rare).await);

        // 488 seconds of 100 records between them: more than one read brings in
        assert_eq!(source.runs(0, 999, 1000), [(10, 11), (500, 500)]);

        // Fewer than one read: read through
        assert_eq!(source.runs(0, 999, 100_000), [(10, 500)]);

        // Without a filter every second may match
        let all = SearchFilter::new(None, None, None);
        let source = source_of(key_index.counts_of(&all).await);
        assert_eq!(source.runs(0, 999, 1000), [(0, 999)]);
    }
}
