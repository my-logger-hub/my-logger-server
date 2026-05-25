use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use rust_extensions::date_time::DateTimeAsMicroseconds;
use tokio::sync::Mutex;
use turso::{params::Params, Builder, Connection, Database, Value};

use super::{dto::*, DateHourKey};

pub const SQLITE_FILE_PREFIX: &str = "logs-";
pub const SQLITE_FILE_SUFFIX: &str = ".db";
const APPLICATION_CTX_KEY: &str = "Application";
const DEBUG_SUBDIR: &str = "debug";
const INFO_SUBDIR: &str = "info";

const CREATE_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS logs (\
    id TEXT NOT NULL,\
    timestamp INTEGER NOT NULL,\
    application TEXT NOT NULL,\
    message TEXT NOT NULL,\
    ctx_json TEXT NOT NULL\
)";

const CREATE_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS idx_app_ts ON logs(application, timestamp)";

const INSERT_SQL: &str =
    "INSERT INTO logs (id, timestamp, application, message, ctx_json) VALUES (?, ?, ?, ?, ?)";

fn level_subdir(level: &LogLevelDto) -> &'static str {
    match level {
        LogLevelDto::Debug => DEBUG_SUBDIR,
        LogLevelDto::Info => INFO_SUBDIR,
        _ => "",
    }
}

fn is_sqlite_level(level: &LogLevelDto) -> bool {
    matches!(level, LogLevelDto::Debug | LogLevelDto::Info)
}

struct HourDb {
    db: Database,
    write_lock: Mutex<()>,
}

impl HourDb {
    async fn open_or_create(path: PathBuf) -> turso::Result<Self> {
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.is_dir() {
                return Err(turso::Error::Misuse(format!(
                    "path '{}' exists and is a directory, expected file",
                    path.display()
                )));
            }
        }
        let path_str = path
            .to_str()
            .ok_or_else(|| turso::Error::Misuse("non-utf8 path".to_string()))?
            .to_string();
        let db = Builder::new_local(&path_str).build().await?;
        let conn = db.connect()?;
        conn.execute(CREATE_TABLE_SQL, ()).await?;
        conn.execute(CREATE_INDEX_SQL, ()).await?;
        Ok(Self {
            db,
            write_lock: Mutex::new(()),
        })
    }

    async fn open_existing(path: PathBuf) -> turso::Result<Option<Self>> {
        match std::fs::metadata(&path) {
            Ok(meta) => {
                if !meta.is_file() {
                    return Ok(None);
                }
            }
            Err(_) => return Ok(None),
        }
        let path_str = path
            .to_str()
            .ok_or_else(|| turso::Error::Misuse("non-utf8 path".to_string()))?
            .to_string();
        let db = Builder::new_local(&path_str).build().await?;
        Ok(Some(Self {
            db,
            write_lock: Mutex::new(()),
        }))
    }

    fn connect(&self) -> turso::Result<Connection> {
        self.db.connect()
    }
}

type PoolKey = (LogLevelDto, DateHourKey);

#[derive(Default)]
struct SqliteLogsRepoPool {
    pool: BTreeMap<PoolKey, Arc<HourDb>>,
    to_delete: Option<PoolKey>,
}

pub struct SqliteLogsRepo {
    pool: Mutex<SqliteLogsRepoPool>,
    path: String,
}

impl SqliteLogsRepo {
    pub fn new(mut path: String) -> Self {
        if path.chars().last().map(|c| c != std::path::MAIN_SEPARATOR).unwrap_or(true) {
            path.push(std::path::MAIN_SEPARATOR);
        }
        let _ = std::fs::create_dir_all(format!("{}{}", path, DEBUG_SUBDIR));
        let _ = std::fs::create_dir_all(format!("{}{}", path, INFO_SUBDIR));
        Self {
            pool: Mutex::new(SqliteLogsRepoPool::default()),
            path,
        }
    }

    pub fn compile_file_name(&self, level: &LogLevelDto, date_key: DateHourKey) -> String {
        let mut s = self.path.clone();
        s.push_str(level_subdir(level));
        s.push(std::path::MAIN_SEPARATOR);
        s.push_str(SQLITE_FILE_PREFIX);
        s.push_str(date_key.get_value().to_string().as_str());
        s.push_str(SQLITE_FILE_SUFFIX);
        s
    }

    async fn get_or_create(&self, level: LogLevelDto, date_key: DateHourKey) -> Arc<HourDb> {
        let key = (level.clone(), date_key);
        {
            let access = self.pool.lock().await;
            if let Some(h) = access.pool.get(&key) {
                return h.clone();
            }
        }

        let path = PathBuf::from(self.compile_file_name(&level, date_key));
        println!("Creating sqlite db: {}", path.display());
        let hour = HourDb::open_or_create(path).await.unwrap();
        let hour = Arc::new(hour);

        let mut access = self.pool.lock().await;
        if let Some(h) = access.pool.get(&key) {
            return h.clone();
        }
        access.pool.insert(key, hour.clone());
        hour
    }

    async fn get_hour(&self, level: LogLevelDto, date_key: DateHourKey) -> Option<Arc<HourDb>> {
        let key = (level.clone(), date_key);
        {
            let access = self.pool.lock().await;
            if let Some(td) = access.to_delete.as_ref() {
                if *td == key {
                    return None;
                }
            }
            if let Some(h) = access.pool.get(&key) {
                return Some(h.clone());
            }
        }

        let path = PathBuf::from(self.compile_file_name(&level, date_key));
        let hour = HourDb::open_existing(path).await.ok()??;
        let hour = Arc::new(hour);

        let mut access = self.pool.lock().await;
        if let Some(h) = access.pool.get(&key) {
            return Some(h.clone());
        }
        access.pool.insert(key, hour.clone());
        Some(hour)
    }

    pub async fn upload(
        &self,
        level: LogLevelDto,
        date_key: DateHourKey,
        items: &[LogItemDto],
    ) {
        if items.is_empty() {
            return;
        }
        let hour = self.get_or_create(level, date_key).await;
        let _guard = hour.write_lock.lock().await;

        let conn = hour.connect().unwrap();
        if let Err(e) = conn.execute("BEGIN", ()).await {
            println!("BEGIN failed: {:?}", e);
            return;
        }

        let mut stmt = match conn.prepare(INSERT_SQL).await {
            Ok(s) => s,
            Err(e) => {
                println!("prepare INSERT failed: {:?}", e);
                let _ = conn.execute("ROLLBACK", ()).await;
                return;
            }
        };

        for item in items {
            let mut ctx = item.context.clone();
            let application = ctx.remove(APPLICATION_CTX_KEY).unwrap_or_default();
            let ctx_json = serde_json::to_string(&ctx).unwrap_or_else(|_| "{}".to_string());

            let params = Params::Positional(vec![
                Value::Text(item.id.clone()),
                Value::Integer(item.moment.unix_microseconds),
                Value::Text(application),
                Value::Text(item.message.clone()),
                Value::Text(ctx_json),
            ]);

            if let Err(e) = stmt.execute(params).await {
                println!("INSERT failed: {:?}", e);
            }
        }

        if let Err(e) = conn.execute("COMMIT", ()).await {
            println!("COMMIT failed: {:?}", e);
            let _ = conn.execute("ROLLBACK", ()).await;
        }
    }

    pub async fn search(
        &self,
        from_date: DateTimeAsMicroseconds,
        to_date: DateTimeAsMicroseconds,
        levels: Option<Vec<LogLevelDto>>,
        context: Option<BTreeMap<String, String>>,
        phrase: Option<&str>,
        limit: usize,
    ) -> Vec<LogItemDto> {
        let levels_to_query = filter_sqlite_levels(levels);
        if levels_to_query.is_empty() {
            return Vec::new();
        }

        let take = if limit == 0 { 1000 } else { limit };

        let mut application_filter: Option<String> = None;
        let mut other_ctx: BTreeMap<String, String> = BTreeMap::new();
        if let Some(ctx) = context {
            for (k, v) in ctx {
                if k == APPLICATION_CTX_KEY {
                    application_filter = Some(v);
                } else {
                    other_ctx.insert(k, v);
                }
            }
        }

        let phrase_lower = phrase
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .map(|p| p.to_lowercase());

        let keys = DateHourKey::get_keys_to_request(from_date, to_date);
        let from_ts = from_date.unix_microseconds;
        let to_ts = to_date.unix_microseconds;

        let mut result: Vec<LogItemDto> = Vec::new();

        'outer: for level in &levels_to_query {
            for date_key in keys.keys().rev() {
                let hour = match self.get_hour(level.clone(), *date_key).await {
                    Some(h) => h,
                    None => continue,
                };

                let rows = query_hour(
                    &hour,
                    level.clone(),
                    from_ts,
                    to_ts,
                    application_filter.as_deref(),
                    phrase_lower.as_deref(),
                    &other_ctx,
                    take,
                )
                .await;

                match rows {
                    Ok(items) => result.extend(items),
                    Err(e) => println!("sqlite query failed: {:?}", e),
                }

                if result.len() >= take {
                    break 'outer;
                }
            }
        }

        result.sort_by(|a, b| b.moment.unix_microseconds.cmp(&a.moment.unix_microseconds));
        result.truncate(take);
        result
    }

    pub async fn get_statistics(&self) -> Vec<StatisticsModel> {
        let now = DateTimeAsMicroseconds::now();
        let date_key: DateHourKey = now.into();

        let mut result = Vec::new();
        for level in [LogLevelDto::Debug, LogLevelDto::Info] {
            let hour = match self.get_hour(level.clone(), date_key).await {
                Some(h) => h,
                None => continue,
            };
            let conn = match hour.connect() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let count = count_rows(&conn).await.unwrap_or(0);
            if count > 0 {
                result.push(StatisticsModel {
                    level,
                    count,
                });
            }
        }
        result
    }

    pub async fn gc(&self, level: LogLevelDto, older_than: DateTimeAsMicroseconds) {
        let cutoff: DateHourKey = older_than.into();
        let subdir = level_subdir(&level);
        if subdir.is_empty() {
            return;
        }
        let dir_path = format!("{}{}", self.path, subdir);

        let mut read_dir = match tokio::fs::read_dir(&dir_path).await {
            Ok(rd) => rd,
            Err(_) => return,
        };

        let mut to_delete: Vec<(DateHourKey, String)> = Vec::new();

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if !name.starts_with(SQLITE_FILE_PREFIX) || !name.ends_with(SQLITE_FILE_SUFFIX) {
                continue;
            }
            let date_str =
                &name[SQLITE_FILE_PREFIX.len()..name.len() - SQLITE_FILE_SUFFIX.len()];
            let date_key = match DateHourKey::parse_from_str(date_str) {
                Some(k) => k,
                None => continue,
            };
            if date_key < cutoff {
                to_delete.push((date_key, name));
            }
        }

        for (date_key, name) in to_delete {
            {
                let mut access = self.pool.lock().await;
                access.to_delete = Some((level.clone(), date_key));
                access.pool.remove(&(level.clone(), date_key));
            }
            let full_path = format!(
                "{}{}{}{}",
                self.path,
                subdir,
                std::path::MAIN_SEPARATOR,
                name
            );
            if let Err(err) = tokio::fs::remove_file(&full_path).await {
                println!("Failed to delete sqlite file {}: {}", full_path, err);
            }
            let _ = tokio::fs::remove_file(format!("{}-wal", full_path)).await;
            let _ = tokio::fs::remove_file(format!("{}-shm", full_path)).await;

            let mut access = self.pool.lock().await;
            access.to_delete = None;
        }
    }
}

fn filter_sqlite_levels(levels: Option<Vec<LogLevelDto>>) -> Vec<LogLevelDto> {
    match levels {
        None => vec![LogLevelDto::Debug, LogLevelDto::Info],
        Some(list) => list.into_iter().filter(is_sqlite_level).collect(),
    }
}

async fn query_hour(
    hour: &HourDb,
    level: LogLevelDto,
    from_ts: i64,
    to_ts: i64,
    application: Option<&str>,
    phrase_lower: Option<&str>,
    other_ctx: &BTreeMap<String, String>,
    take: usize,
) -> turso::Result<Vec<LogItemDto>> {
    let mut sql = String::from(
        "SELECT id, timestamp, application, message, ctx_json FROM logs \
         WHERE timestamp BETWEEN ? AND ?",
    );
    let mut params: Vec<Value> = vec![Value::Integer(from_ts), Value::Integer(to_ts)];

    if let Some(app) = application {
        sql.push_str(" AND application = ?");
        params.push(Value::Text(app.to_string()));
    }
    if let Some(p) = phrase_lower {
        sql.push_str(" AND LOWER(message) LIKE ?");
        let mut pat = String::with_capacity(p.len() + 2);
        pat.push('%');
        pat.push_str(p);
        pat.push('%');
        params.push(Value::Text(pat));
    }
    sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
    params.push(Value::Integer(take as i64));

    let conn = hour.connect()?;
    let mut rows = conn.query(&sql, Params::Positional(params)).await?;

    let mut out = Vec::with_capacity(take.min(256));
    while let Some(row) = rows.next().await? {
        let id = match row.get_value(0)? {
            Value::Text(s) => s,
            _ => String::new(),
        };
        let ts = match row.get_value(1)? {
            Value::Integer(i) => i,
            _ => 0,
        };
        let application = match row.get_value(2)? {
            Value::Text(s) => s,
            _ => String::new(),
        };
        let message = match row.get_value(3)? {
            Value::Text(s) => s,
            _ => String::new(),
        };
        let ctx_json = match row.get_value(4)? {
            Value::Text(s) => s,
            _ => "{}".to_string(),
        };

        let mut context: BTreeMap<String, String> =
            serde_json::from_str(&ctx_json).unwrap_or_default();
        if !application.is_empty() {
            context.insert(APPLICATION_CTX_KEY.to_string(), application);
        }

        if !other_ctx_matches(other_ctx, &context) {
            continue;
        }

        out.push(LogItemDto {
            moment: DateTimeAsMicroseconds::new(ts),
            id,
            level: level.clone(),
            message,
            context,
        });
    }
    Ok(out)
}

fn other_ctx_matches(
    required: &BTreeMap<String, String>,
    found: &BTreeMap<String, String>,
) -> bool {
    for (k, v) in required {
        match found.get(k) {
            Some(actual) if actual.eq_ignore_ascii_case(v) => continue,
            _ => return false,
        }
    }
    true
}

async fn count_rows(conn: &Connection) -> turso::Result<i64> {
    let mut rows = conn.query("SELECT COUNT(*) FROM logs", ()).await?;
    if let Some(row) = rows.next().await? {
        if let Value::Integer(c) = row.get_value(0)? {
            return Ok(c);
        }
    }
    Ok(0)
}

