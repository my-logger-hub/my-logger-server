use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use my_s3::{S3Client, S3ListObjectsRequest};
use rust_extensions::AsyncBytesStream;
use tokio::io::AsyncWriteExt;

use crate::{repo::DateHourKey, settings::S3ConnectionSettings};

use super::{
    data_file_name, index_file_name, key_index_file_name, scoped_name, DataSource, DataStream,
    HourCells, KeyIndexDirectory, S3ObjectStream,
};

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The folder of the hours in the bucket when the settings give none
const DEFAULT_PREFIX: &str = "logs";

/// An hour found missing in S3 is asked for again after this
const MISSING_HOUR_TTL: Duration = Duration::from_secs(60);

/// Indexes of hours in S3 kept in memory
const INDEX_CACHE_SIZE: usize = 64;

/// The hours of a scope the bucket lists are listed again after this
const LISTED_HOURS_TTL: Duration = Duration::from_secs(60);

/// The index of an hour in S3 and the page of keys and values of its key index
pub type HourIndexes = (Arc<HourCells>, Arc<KeyIndexDirectory>);

struct CachedIndex {
    indexes: Option<HourIndexes>,
    loaded: Instant,
}

struct ListedHours {
    hours: Arc<BTreeSet<DateHourKey>>,
    loaded: Instant,
}

/// The hours of the Main stream once they are uploaded: `{prefix}/YYYYMMDDHH.data` and
/// `.index`, the same files as on the local disk, their frames one after another.
pub struct S3Archive {
    client: S3Client,
    bucket: String,
    /// Ends with '/'
    prefix: String,
    /// By product scope and hour
    indexes: Mutex<BTreeMap<(String, DateHourKey), CachedIndex>>,
    /// The hours each product scope has an index of. Locked while the bucket is listed: an
    /// upload or a delete which ends meanwhile changes what the listing brought
    listed_hours: tokio::sync::Mutex<BTreeMap<String, ListedHours>>,
    bucket_is_ready: AtomicBool,
}

impl S3Archive {
    pub fn new(settings: &S3ConnectionSettings) -> Self {
        let prefix = settings
            .prefix
            .as_deref()
            .map(|prefix| prefix.trim_matches('/'))
            .filter(|prefix| !prefix.is_empty())
            .unwrap_or(DEFAULT_PREFIX);

        let client = S3Client::new(
            settings.access_key.as_str(),
            settings.secret_key.as_str(),
            settings.region.as_str(),
            settings.endpoint.as_str(),
        );

        // `Debug=1`: every request is traced to stdout - the verb, the url and the body size
        // going out, the whole answer body when it failed. The `Authorization` header is not.
        let client = if settings.debug {
            println!("S3 request tracing is ON (Debug in s3_conn_string)");
            client.debug_to_console()
        } else {
            client
        };

        Self {
            client,
            bucket: settings.bucket.clone(),
            prefix: format!("{}/", prefix),
            indexes: Mutex::new(BTreeMap::new()),
            listed_hours: tokio::sync::Mutex::new(BTreeMap::new()),
            bucket_is_ready: AtomicBool::new(false),
        }
    }

    fn key(&self, file_name: String) -> String {
        format!("{}{}", self.prefix, file_name)
    }

    /// The index of an hour of the scope and the page of its key index; `None` - the hour
    /// is not in S3
    pub async fn get_indexes(
        archive: &Arc<S3Archive>,
        scope: &str,
        hour: DateHourKey,
    ) -> Result<Option<HourIndexes>, String> {
        let this = archive.as_ref();
        let cache_key = (scope.to_string(), hour);

        if let Some(cached) = this.indexes.lock().unwrap().get(&cache_key) {
            if cached.indexes.is_some() || cached.loaded.elapsed() < MISSING_HOUR_TTL {
                return Ok(cached.indexes.clone());
            }
        }

        let indexes_of_hour = match this.download(scoped_name(scope, index_file_name(hour))).await? {
            Some(bytes) => {
                let cells = HourCells::from_index_file(&bytes)
                    .ok_or_else(|| format!("The index of {} in S3 is not an index", hour.get_value()))?;

                let source = DataSource::S3 {
                    archive: archive.clone(),
                    file_name: scoped_name(scope, key_index_file_name(hour)),
                };

                // Without a key index every search of the hour scans it
                let directory = match KeyIndexDirectory::read(&source).await {
                    Ok(directory) => directory,
                    Err(err) => {
                        println!("The key index of {} in S3: {}", hour.get_value(), err);
                        KeyIndexDirectory::empty()
                    }
                };

                Some((Arc::new(cells), Arc::new(directory)))
            }
            None => None,
        };

        let mut indexes = this.indexes.lock().unwrap();

        if indexes.len() >= INDEX_CACHE_SIZE {
            let oldest = indexes
                .iter()
                .min_by_key(|(_, cached)| cached.loaded)
                .map(|(key, _)| key.clone());

            if let Some(oldest) = oldest {
                indexes.remove(&oldest);
            }
        }

        indexes.insert(
            cache_key,
            CachedIndex {
                indexes: indexes_of_hour.clone(),
                loaded: Instant::now(),
            },
        );

        Ok(indexes_of_hour)
    }

    /// The hours of the scope which are in S3: the ones the bucket lists an index of
    pub async fn hours(&self, scope: &str) -> Result<Arc<BTreeSet<DateHourKey>>, String> {
        let mut listed_hours = self.listed_hours.lock().await;

        if let Some(listed) = listed_hours.get(scope) {
            if listed.loaded.elapsed() < LISTED_HOURS_TTL {
                return Ok(listed.hours.clone());
            }
        }

        let scope_prefix = self.key(scoped_name(scope, String::new()));

        let hours: BTreeSet<DateHourKey> = self
            .list_keys(&scope_prefix)
            .await?
            .iter()
            .filter_map(|key| key.strip_prefix(scope_prefix.as_str())?.strip_suffix(".index"))
            .filter_map(DateHourKey::parse_from_str)
            .collect();

        let hours = Arc::new(hours);

        listed_hours.insert(
            scope.to_string(),
            ListedHours {
                hours: hours.clone(),
                loaded: Instant::now(),
            },
        );

        Ok(hours)
    }

    /// Every key of the bucket which starts with `prefix`
    async fn list_keys(&self, prefix: &str) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let page = self
                .client
                .list_objects_v2(
                    &self.bucket,
                    S3ListObjectsRequest {
                        prefix: Some(prefix),
                        continuation_token: continuation_token.as_deref(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|err| format!("Listing of {}: {}", self.bucket, err))?;

            keys.extend(page.objects.into_iter().map(|object| object.key));

            continuation_token = page.next_continuation_token;

            if continuation_token.is_none() {
                break;
            }
        }

        Ok(keys)
    }

    /// A whole object; `None` - there is no such
    async fn download(&self, file_name: String) -> Result<Option<Vec<u8>>, String> {
        let key = self.key(file_name);

        match self.client.download_file(&self.bucket, &key).await {
            Ok(stream) => Ok(Some(S3ObjectStream::new(stream).into_vec().await?)),
            Err(err) if err.is_key_not_found() => Ok(None),
            Err(err) => Err(format!("{}: {}", key, err)),
        }
    }

    /// A file of an hour from `offset` on, to `end` inclusive when it is given
    pub async fn open_stream(
        &self,
        file_name: &str,
        offset: u64,
        end: Option<u64>,
    ) -> Result<DataStream, String> {
        let key = self.key(file_name.to_string());

        let stream = self
            .client
            .download_file_range(&self.bucket, &key, offset, end)
            .await
            .map_err(|err| format!("{}: {}", key, err))?;

        Ok(Arc::new(S3ObjectStream::new(stream)))
    }

    /// Uploads the data files - `data_paths[n]` as `.dataNN` - checking the size of each,
    /// then the key index and the index: an hour is in S3 once its index is.
    pub async fn upload(
        &self,
        scope: &str,
        hour: DateHourKey,
        data_paths: &[PathBuf],
        index_path: &Path,
        key_index_path: &Path,
    ) -> Result<(), String> {
        if !self.bucket_is_ready.load(Ordering::Relaxed) {
            // Created only when it is not there: keys allowed to write may not be allowed
            // to create a bucket
            match self.client.check_if_bucket_exists(&self.bucket).await {
                Ok(true) => {}
                Ok(false) => self
                    .client
                    .create_bucket_if_not_exists(&self.bucket)
                    .await
                    .map_err(|err| format!("Bucket {}: {}", self.bucket, err))?,
                Err(err) => println!(
                    "Bucket {} is not checked: {}. The upload says whether it is there",
                    self.bucket, err
                ),
            }

            self.bucket_is_ready.store(true, Ordering::Relaxed);
        }

        for (file, data_path) in data_paths.iter().enumerate() {
            self.upload_data_file(data_path, self.key(scoped_name(scope, data_file_name(hour, file as u8))))
                .await?;
        }

        for (path, file_name) in [
            (key_index_path, scoped_name(scope, key_index_file_name(hour))),
            (index_path, scoped_name(scope, index_file_name(hour))),
        ] {
            let content = tokio::fs::read(path)
                .await
                .map_err(|err| format!("{}: {}", path.display(), err))?;

            let key = self.key(file_name);

            self.client
                .upload(&self.bucket, &key, content, UPLOAD_TIMEOUT)
                .await
                .map_err(|err| format!("Upload of {}: {}", key, err))?;
        }

        self.indexes.lock().unwrap().remove(&(scope.to_string(), hour));

        if let Some(listed) = self.listed_hours.lock().await.get_mut(scope) {
            Arc::make_mut(&mut listed.hours).insert(hour);
        }

        Ok(())
    }

    /// Streams the file to S3 and checks the size it has there
    async fn upload_data_file(&self, path: &Path, key: String) -> Result<(), String> {
        let len = tokio::fs::metadata(path)
            .await
            .map_err(|err| format!("{}: {}", path.display(), err))?
            .len();

        let path = path.to_path_buf();

        self.client
            .upload_with_writer(
                &self.bucket,
                &key,
                len as usize,
                UPLOAD_TIMEOUT,
                |mut writer| async move {
                    let mut file = tokio::fs::File::open(&path).await?;
                    tokio::io::copy(&mut file, &mut writer).await?;
                    writer.shutdown().await
                },
            )
            .await
            .map_err(|err| format!("Upload of {}: {}", key, err))?;

        let uploaded = self
            .client
            .get_object_size(&self.bucket, &key)
            .await
            .map_err(|err| format!("{}: {}", key, err))?;

        if uploaded != len {
            return Err(format!(
                "{} is {} bytes in S3, and {} bytes were uploaded",
                key, uploaded, len
            ));
        }

        Ok(())
    }

    /// Deletes every hour up to `cutoff` - every object of it the bucket lists: its index
    /// first, so an hour deleted halfway is not read
    pub async fn delete_up_to(
        &self,
        cutoff: DateHourKey,
    ) -> Result<Vec<(String, DateHourKey)>, String> {
        let mut hours: BTreeMap<(String, DateHourKey), Vec<String>> = BTreeMap::new();

        for key in self.list_keys(self.prefix.as_str()).await? {
            // `scope/YYYYMMDDHH.` and what the file of the hour is
            let scope_and_hour = key
                .strip_prefix(self.prefix.as_str())
                .and_then(|name| name.split_once('/'))
                .filter(|(_, name)| name.get(10..11) == Some("."))
                .and_then(|(scope, name)| {
                    let hour = DateHourKey::parse_from_str(name.get(..10)?)?;
                    Some((scope.to_string(), hour))
                });

            if let Some((scope, hour)) = scope_and_hour {
                if hour <= cutoff {
                    hours.entry((scope, hour)).or_default().push(key);
                }
            }
        }

        for ((scope, hour), keys) in hours.iter_mut() {
            let mut keys = std::mem::take(keys);
            let index_key = self.key(scoped_name(scope, index_file_name(*hour)));
            keys.sort_by_key(|key| *key != index_key);

            for key in keys {
                self.client
                    .delete_file(&self.bucket, &key)
                    .await
                    .map_err(|err| format!("Delete of {}: {}", key, err))?;
            }

            self.indexes.lock().unwrap().remove(&(scope.clone(), *hour));

            if let Some(listed) = self.listed_hours.lock().await.get_mut(scope) {
                Arc::make_mut(&mut listed.hours).remove(hour);
            }
        }

        Ok(hours.into_keys().collect())
    }
}
