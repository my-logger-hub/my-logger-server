use std::{
    collections::BTreeMap,
    ops::Bound,
    path::PathBuf,
    sync::Arc,
};

use rust_extensions::date_time::DateTimeAsMicroseconds;
use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery},
    schema::{
        Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, INDEXED,
        STORED,
    },
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term,
};
use tokio::sync::Mutex;

use crate::app::PROCESS_CONTEXT_KEY;

use super::{dto::*, DateHourKey};

pub const LOG_FILE_PREFIX: &str = "logs-";
const WRITER_HEAP: usize = 50_000_000;

const F_TIMESTAMP: &str = "timestamp";
const F_ID: &str = "id";
const F_LEVEL: &str = "level";
const F_MESSAGE: &str = "message";
const F_CTX: &str = "ctx";
const F_CTX_DATA: &str = "ctx_data";

#[derive(Clone)]
struct SchemaFields {
    timestamp: Field,
    id: Field,
    level: Field,
    message: Field,
    ctx: Field,
    ctx_data: Field,
}

fn build_schema() -> (Schema, SchemaFields) {
    let mut sb = Schema::builder();

    let timestamp = sb.add_i64_field(F_TIMESTAMP, INDEXED | FAST | STORED);
    let id = sb.add_text_field(F_ID, STORED);

    let raw_indexed = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("raw")
                .set_index_option(IndexRecordOption::Basic),
        )
        .set_stored();

    let level = sb.add_text_field(F_LEVEL, raw_indexed.clone());

    let message_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let message = sb.add_text_field(F_MESSAGE, message_options);

    let ctx_indexed_only = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("raw")
            .set_index_option(IndexRecordOption::Basic),
    );
    let ctx = sb.add_text_field(F_CTX, ctx_indexed_only);

    let ctx_data = sb.add_text_field(F_CTX_DATA, STORED);

    let schema = sb.build();
    (
        schema,
        SchemaFields {
            timestamp,
            id,
            level,
            message,
            ctx,
            ctx_data,
        },
    )
}

fn level_term(level: &LogLevelDto) -> &'static str {
    match level {
        LogLevelDto::Info => "info",
        LogLevelDto::Warning => "warning",
        LogLevelDto::Error => "error",
        LogLevelDto::FatalError => "fatalerror",
        LogLevelDto::Debug => "debug",
    }
}

fn parse_level(s: &str) -> LogLevelDto {
    match s {
        "info" => LogLevelDto::Info,
        "warning" => LogLevelDto::Warning,
        "error" => LogLevelDto::Error,
        "fatalerror" => LogLevelDto::FatalError,
        "debug" => LogLevelDto::Debug,
        _ => LogLevelDto::Info,
    }
}

fn ctx_token(key: &str, value: &str) -> String {
    let mut s = String::with_capacity(key.len() + value.len() + 1);
    s.push_str(&key.to_lowercase());
    s.push('=');
    s.push_str(&value.to_lowercase());
    s
}

struct HourIndex {
    index: Index,
    fields: SchemaFields,
    reader: IndexReader,
    write_lock: Mutex<()>,
}

impl HourIndex {
    fn open_or_create(path: PathBuf) -> tantivy::Result<Self> {
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.is_file() {
                println!(
                    "Removing legacy SQLite shard at {}",
                    path.display()
                );
                std::fs::remove_file(&path)
                    .map_err(|e| tantivy::TantivyError::IoError(Arc::new(e)))?;
            }
        }
        std::fs::create_dir_all(&path)
            .map_err(|e| tantivy::TantivyError::IoError(Arc::new(e)))?;
        let (schema, fields) = build_schema();
        let dir = MmapDirectory::open(&path)?;
        let index = Index::open_or_create(dir, schema)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        Ok(Self {
            index,
            fields,
            reader,
            write_lock: Mutex::new(()),
        })
    }

    fn open_existing(path: PathBuf) -> tantivy::Result<Option<Self>> {
        match std::fs::metadata(&path) {
            Ok(meta) => {
                if !meta.is_dir() {
                    return Ok(None);
                }
            }
            Err(_) => return Ok(None),
        }
        let (schema, fields) = build_schema();
        let dir = MmapDirectory::open(&path)?;
        let index = Index::open_or_create(dir, schema)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        Ok(Some(Self {
            index,
            fields,
            reader,
            write_lock: Mutex::new(()),
        }))
    }
}

#[derive(Default)]
struct LogsRepoPool {
    pool: BTreeMap<DateHourKey, Arc<HourIndex>>,
    to_delete: Option<DateHourKey>,
}

pub struct LogsRepo {
    pool: Mutex<LogsRepoPool>,
    path: String,
}

impl LogsRepo {
    pub async fn new(mut path: String) -> Self {
        if path.chars().last().unwrap() != std::path::MAIN_SEPARATOR {
            path.push(std::path::MAIN_SEPARATOR);
        }
        let _ = tokio::fs::create_dir_all(&path).await;
        Self {
            pool: Mutex::new(LogsRepoPool::default()),
            path,
        }
    }

    pub fn compile_file_name(&self, date_key: DateHourKey) -> String {
        let mut path = self.path.clone();
        path.push_str(LOG_FILE_PREFIX);
        path.push_str(date_key.get_value().to_string().as_str());
        path
    }

    async fn get_or_create(&self, date_key: DateHourKey) -> Arc<HourIndex> {
        let mut access = self.pool.lock().await;
        if let Some(idx) = access.pool.get(&date_key) {
            return idx.clone();
        }
        let path = PathBuf::from(self.compile_file_name(date_key));
        println!("Creating tantivy index: {}", path.display());
        let idx = tokio::task::spawn_blocking(move || HourIndex::open_or_create(path))
            .await
            .unwrap()
            .unwrap();
        let idx = Arc::new(idx);
        access.pool.insert(date_key, idx.clone());
        idx
    }

    async fn get_hour(&self, date_key: DateHourKey) -> Option<Arc<HourIndex>> {
        let mut access = self.pool.lock().await;
        if let Some(td) = access.to_delete.as_ref() {
            if td.get_value() == date_key.get_value() {
                return None;
            }
        }
        if let Some(idx) = access.pool.get(&date_key) {
            return Some(idx.clone());
        }
        let path = PathBuf::from(self.compile_file_name(date_key));
        let idx = tokio::task::spawn_blocking(move || HourIndex::open_existing(path))
            .await
            .ok()?
            .ok()?;
        let idx = idx?;
        let arc = Arc::new(idx);
        access.pool.insert(date_key, arc.clone());
        Some(arc)
    }

    async fn get_last(&self) -> Option<Arc<HourIndex>> {
        let now = DateTimeAsMicroseconds::now();
        let date_key: DateHourKey = now.into();
        self.get_hour(date_key).await
    }

    async fn get_last_and_before(&self) -> Vec<Arc<HourIndex>> {
        let now = DateTimeAsMicroseconds::now();
        let date_key_now: DateHourKey = now.into();
        let before = now.sub(std::time::Duration::from_secs(60 * 60));
        let date_key_before: DateHourKey = before.into();

        let mut result = Vec::with_capacity(2);
        if let Some(idx) = self.get_hour(date_key_now).await {
            result.push(idx);
        }
        if let Some(idx) = self.get_hour(date_key_before).await {
            result.push(idx);
        }
        result
    }

    pub async fn upload(&self, date_key: DateHourKey, items: &[LogItemDto]) {
        if items.is_empty() {
            return;
        }
        let hour = self.get_or_create(date_key).await;
        let _guard = hour.write_lock.lock().await;
        let index = hour.index.clone();
        let fields = hour.fields.clone();
        let items_owned: Vec<LogItemDto> = items.to_vec();

        tokio::task::spawn_blocking(move || -> tantivy::Result<()> {
            let mut writer: IndexWriter<TantivyDocument> = index.writer(WRITER_HEAP)?;
            for item in items_owned {
                let mut doc = TantivyDocument::default();
                doc.add_i64(fields.timestamp, item.moment.unix_microseconds);
                doc.add_text(fields.id, &item.id);
                doc.add_text(fields.level, level_term(&item.level));
                doc.add_text(fields.message, &item.message);
                for (k, v) in &item.context {
                    doc.add_text(fields.ctx, ctx_token(k, v));
                }
                let ctx_json = serde_json::to_string(&item.context).unwrap_or_default();
                doc.add_text(fields.ctx_data, ctx_json);
                writer.add_document(doc)?;
            }
            writer.commit()?;
            Ok(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    pub async fn get_from_certain_hour(
        &self,
        date_key: DateHourKey,
        levels: Option<Vec<LogLevelDto>>,
        context: Option<BTreeMap<String, String>>,
        take: usize,
    ) -> Vec<LogItemDto> {
        let hour = match self.get_hour(date_key).await {
            Some(h) => h,
            None => return Vec::new(),
        };
        search_hour(&hour, None, None, levels.as_deref(), context.as_ref(), None, take)
            .await
            .unwrap_or_default()
    }

    pub async fn get(
        &self,
        from_date: DateTimeAsMicroseconds,
        to_date: Option<DateTimeAsMicroseconds>,
        levels: Option<Vec<LogLevelDto>>,
        context: Option<BTreeMap<String, String>>,
        take: usize,
    ) -> Vec<LogItemDto> {
        let to_date = to_date.unwrap_or_else(DateTimeAsMicroseconds::now);
        let keys = DateHourKey::get_keys_to_request(from_date, to_date);

        println!("Files to request: {:?}", keys.keys().collect::<Vec<_>>());

        let mut result: Vec<LogItemDto> = Vec::new();
        for date_key in keys.keys().rev() {
            let hour = match self.get_hour(*date_key).await {
                Some(h) => h,
                None => continue,
            };
            match search_hour(
                &hour,
                Some(from_date.unix_microseconds),
                Some(to_date.unix_microseconds),
                levels.as_deref(),
                context.as_ref(),
                None,
                take,
            )
            .await
            {
                Ok(items) => result.extend(items),
                Err(e) => println!("Error: {:?}", e),
            }
            if result.len() >= take {
                break;
            }
        }
        result.truncate(take);
        result
    }

    pub async fn scan(
        &self,
        from_date: DateTimeAsMicroseconds,
        to_date: DateTimeAsMicroseconds,
        phrase: &str,
        limit: usize,
        debug: bool,
    ) -> Vec<LogItemDto> {
        let keys = DateHourKey::get_keys_to_request(from_date, to_date);

        let mut result: Vec<LogItemDto> = Vec::new();
        for date_key in keys.keys().rev() {
            if debug {
                println!(
                    "Requesting search from index:'{}'. From: {}, to: {}",
                    date_key.get_value(),
                    from_date.to_rfc3339(),
                    to_date.to_rfc3339()
                );
            }
            let hour = match self.get_hour(*date_key).await {
                Some(h) => h,
                None => continue,
            };
            match search_hour(
                &hour,
                Some(from_date.unix_microseconds),
                Some(to_date.unix_microseconds),
                None,
                None,
                Some(phrase),
                limit,
            )
            .await
            {
                Ok(items) => result.extend(items),
                Err(e) => println!("Error: {:?}", e),
            }
            if result.len() >= limit {
                break;
            }
        }
        result.truncate(limit);
        result
    }

    pub async fn scan_from_exact_hour(
        &self,
        hour_key: DateHourKey,
        phrase: &str,
        limit: usize,
    ) -> Vec<LogItemDto> {
        let hour = match self.get_hour(hour_key).await {
            Some(h) => h,
            None => {
                println!("No tantivy index for hour: {}", hour_key.get_value());
                return Vec::new();
            }
        };
        println!("Doing scan from exact hour: {}", hour_key.get_value());
        search_hour(&hour, None, None, None, None, Some(phrase), limit)
            .await
            .unwrap_or_default()
    }

    pub async fn prepare_to_delete(&self, date_key: DateHourKey) {
        let mut access = self.pool.lock().await;
        access.to_delete = Some(date_key);
        access.pool.remove(&date_key);
    }

    pub async fn get_statistics(&self) -> Vec<StatisticsModel> {
        let hour = match self.get_last().await {
            Some(h) => h,
            None => return Vec::new(),
        };
        let fields = hour.fields.clone();
        let reader = hour.reader.clone();
        tokio::task::spawn_blocking(move || -> tantivy::Result<Vec<StatisticsModel>> {
            let searcher = reader.searcher();
            let levels = [
                LogLevelDto::Info,
                LogLevelDto::Warning,
                LogLevelDto::Error,
                LogLevelDto::FatalError,
                LogLevelDto::Debug,
            ];
            let mut result = Vec::with_capacity(levels.len());
            for lvl in levels {
                let term = Term::from_field_text(fields.level, level_term(&lvl));
                let q = TermQuery::new(term, IndexRecordOption::Basic);
                let count = searcher.search(&q, &tantivy::collector::Count)?;
                if count > 0 {
                    result.push(StatisticsModel {
                        level: lvl,
                        count: count as i64,
                    });
                }
            }
            Ok(result)
        })
        .await
        .unwrap()
        .unwrap_or_default()
    }

    pub async fn get_files(&self) -> Vec<String> {
        let mut files = Vec::new();
        let mut read_dir = match tokio::fs::read_dir(self.path.as_str()).await {
            Ok(rd) => rd,
            Err(_) => return files,
        };
        while let Ok(Some(dir_entry)) = read_dir.next_entry().await {
            if let Ok(name) = dir_entry.file_name().into_string() {
                files.push(name);
            }
        }
        files
    }

    pub async fn gc(&self, to_date: DateTimeAsMicroseconds) -> Vec<DateHourKey> {
        let gc_from = DateHourKey::new(to_date);

        let mut access = self.pool.lock().await;

        let mut to_gc = Vec::new();
        for date_key in access.pool.keys() {
            if date_key < &gc_from {
                to_gc.push(*date_key);
            }
        }

        for to_gc in &to_gc {
            access.pool.remove(to_gc);
        }

        to_gc
    }

    pub async fn gc_level(&self, to_date: DateTimeAsMicroseconds, level: LogLevelDto) {
        let last_and_before = self.get_last_and_before().await;
        for hour in last_and_before {
            let _guard = hour.write_lock.lock().await;
            let index = hour.index.clone();
            let fields = hour.fields.clone();
            let level = level.clone();
            let to_micros = to_date.unix_microseconds;
            let _ = tokio::task::spawn_blocking(move || -> tantivy::Result<()> {
                let mut writer: IndexWriter<TantivyDocument> = index.writer(WRITER_HEAP)?;
                let range = RangeQuery::new(
                    Bound::Unbounded,
                    Bound::Included(Term::from_field_i64(fields.timestamp, to_micros)),
                );
                let level_q = TermQuery::new(
                    Term::from_field_text(fields.level, level_term(&level)),
                    IndexRecordOption::Basic,
                );
                let q = BooleanQuery::new(vec![
                    (Occur::Must, Box::new(range) as Box<dyn Query>),
                    (Occur::Must, Box::new(level_q) as Box<dyn Query>),
                ]);
                writer.delete_query(Box::new(q))?;
                writer.commit()?;
                Ok(())
            })
            .await;
        }
    }
}

async fn search_hour(
    hour: &HourIndex,
    from_ts: Option<i64>,
    to_ts: Option<i64>,
    levels: Option<&[LogLevelDto]>,
    context: Option<&BTreeMap<String, String>>,
    phrase: Option<&str>,
    take: usize,
) -> tantivy::Result<Vec<LogItemDto>> {
    let take = if take == 0 { 1000 } else { take };
    let fields = hour.fields.clone();
    let reader = hour.reader.clone();
    let index = hour.index.clone();

    let levels_owned: Vec<String> = levels
        .map(|l| l.iter().map(|x| level_term(x).to_string()).collect())
        .unwrap_or_default();
    let ctx_owned: Vec<String> = context
        .map(|c| c.iter().map(|(k, v)| ctx_token(k, v)).collect())
        .unwrap_or_default();
    let phrase_owned = phrase.map(|s| s.to_string());

    tokio::task::spawn_blocking(move || -> tantivy::Result<Vec<LogItemDto>> {
        let searcher = reader.searcher();

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        if from_ts.is_some() || to_ts.is_some() {
            let lower = match from_ts {
                Some(v) => Bound::Included(Term::from_field_i64(fields.timestamp, v)),
                None => Bound::Unbounded,
            };
            let upper = match to_ts {
                Some(v) => Bound::Included(Term::from_field_i64(fields.timestamp, v)),
                None => Bound::Unbounded,
            };
            clauses.push((
                Occur::Must,
                Box::new(RangeQuery::new(lower, upper)) as Box<dyn Query>,
            ));
        }

        if !levels_owned.is_empty() {
            let mut sub: Vec<(Occur, Box<dyn Query>)> = Vec::new();
            for l in &levels_owned {
                sub.push((
                    Occur::Should,
                    Box::new(TermQuery::new(
                        Term::from_field_text(fields.level, l),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                ));
            }
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(sub)) as Box<dyn Query>));
        }

        for kv in &ctx_owned {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(fields.ctx, kv),
                    IndexRecordOption::Basic,
                )) as Box<dyn Query>,
            ));
        }

        if let Some(p) = &phrase_owned {
            let qp = QueryParser::for_index(&index, vec![fields.message]);
            if let Ok(q) = qp.parse_query(p) {
                clauses.push((Occur::Must, q));
            }
        }

        let query: Box<dyn Query> = if clauses.is_empty() {
            Box::new(tantivy::query::AllQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

        let collector = TopDocs::with_limit(take)
            .order_by_fast_field::<i64>(F_TIMESTAMP, tantivy::Order::Desc);
        let docs = searcher.search(&*query, &collector)?;

        let mut out = Vec::with_capacity(docs.len());
        for (_, addr) in docs {
            let retrieved = searcher.doc::<TantivyDocument>(addr)?;
            out.push(doc_to_dto(&retrieved, &fields));
        }
        Ok(out)
    })
    .await
    .unwrap()
}

fn doc_to_dto(doc: &TantivyDocument, fields: &SchemaFields) -> LogItemDto {
    let timestamp = doc
        .get_first(fields.timestamp)
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let id = doc
        .get_first(fields.id)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let level_s = doc
        .get_first(fields.level)
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let message = doc
        .get_first(fields.message)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ctx_data = doc
        .get_first(fields.ctx_data)
        .and_then(|v| v.as_str())
        .unwrap_or("{}");
    let context: BTreeMap<String, String> =
        serde_json::from_str(ctx_data).unwrap_or_default();

    LogItemDto {
        moment: DateTimeAsMicroseconds::new(timestamp),
        id,
        level: parse_level(level_s),
        message,
        context,
    }
}

#[allow(dead_code)]
fn _process_key_marker() -> &'static str {
    PROCESS_CONTEXT_KEY
}
