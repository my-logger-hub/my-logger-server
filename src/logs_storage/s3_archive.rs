use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use my_s3::{S3Client, S3ListObjectsRequest};
use rust_extensions::AsyncBytesStream;
use tokio::io::AsyncWriteExt;

use crate::{repo::DateHourKey, settings::S3Settings};

use super::{
    data_file_name, index_file_name, key_index_file_name, DataSource, DataStream, HourCells,
    KeyIndexDirectory, S3ObjectStream,
};

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// An hour found missing in S3 is asked for again after this
const MISSING_HOUR_TTL: Duration = Duration::from_secs(60);

/// Indexes of hours in S3 kept in memory
const INDEX_CACHE_SIZE: usize = 64;

/// The index of an hour in S3 and the page of keys and values of its key index
pub type HourIndexes = (Arc<HourCells>, Arc<KeyIndexDirectory>);

struct CachedIndex {
    indexes: Option<HourIndexes>,
    loaded: Instant,
}

/// The hours of the Main stream once they are uploaded: `{prefix}/YYYYMMDDHH.data` and
/// `.index`, the same files as on the local disk, their frames one after another.
pub struct S3Archive {
    client: S3Client,
    bucket: String,
    /// Empty, or ends with '/'
    prefix: String,
    indexes: Mutex<BTreeMap<DateHourKey, CachedIndex>>,
    bucket_is_ready: AtomicBool,
}

impl S3Archive {
    pub fn new(settings: &S3Settings, env_name: &str) -> Self {
        let prefix = settings.prefix.as_deref().unwrap_or(env_name).trim_matches('/');

        Self {
            client: S3Client::new(
                settings.access_key.as_str(),
                settings.secret_key.as_str(),
                settings.region.as_str(),
                settings.endpoint.as_str(),
            ),
            bucket: settings.bucket.clone(),
            prefix: if prefix.is_empty() {
                String::new()
            } else {
                format!("{}/", prefix)
            },
            indexes: Mutex::new(BTreeMap::new()),
            bucket_is_ready: AtomicBool::new(false),
        }
    }

    fn key(&self, file_name: String) -> String {
        format!("{}{}", self.prefix, file_name)
    }

    /// The index of an hour and the page of its key index; `None` - the hour is not in S3
    pub async fn get_indexes(
        archive: &Arc<S3Archive>,
        hour: DateHourKey,
    ) -> Result<Option<HourIndexes>, String> {
        let this = archive.as_ref();

        if let Some(cached) = this.indexes.lock().unwrap().get(&hour) {
            if cached.indexes.is_some() || cached.loaded.elapsed() < MISSING_HOUR_TTL {
                return Ok(cached.indexes.clone());
            }
        }

        let indexes_of_hour = match this.download(index_file_name(hour)).await? {
            Some(bytes) => {
                let cells = HourCells::from_index_file(&bytes)
                    .ok_or_else(|| format!("The index of {} in S3 is not an index", hour.get_value()))?;

                let source = DataSource::S3 {
                    archive: archive.clone(),
                    file_name: key_index_file_name(hour),
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
                .map(|(hour, _)| *hour);

            if let Some(oldest) = oldest {
                indexes.remove(&oldest);
            }
        }

        indexes.insert(
            hour,
            CachedIndex {
                indexes: indexes_of_hour.clone(),
                loaded: Instant::now(),
            },
        );

        Ok(indexes_of_hour)
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

    /// Uploads the data file, checks its size, then uploads the key index and the index:
    /// an hour is in S3 once its index is.
    pub async fn upload(
        &self,
        hour: DateHourKey,
        data_path: &Path,
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

        let data_key = self.key(data_file_name(hour));
        let data_len = tokio::fs::metadata(data_path)
            .await
            .map_err(|err| format!("{}: {}", data_path.display(), err))?
            .len();

        let path = data_path.to_path_buf();

        self.client
            .upload_with_writer(
                &self.bucket,
                &data_key,
                data_len as usize,
                UPLOAD_TIMEOUT,
                |mut writer| async move {
                    let mut file = tokio::fs::File::open(&path).await?;
                    tokio::io::copy(&mut file, &mut writer).await?;
                    writer.shutdown().await
                },
            )
            .await
            .map_err(|err| format!("Upload of {}: {}", data_key, err))?;

        let uploaded = self
            .client
            .get_object_size(&self.bucket, &data_key)
            .await
            .map_err(|err| format!("{}: {}", data_key, err))?;

        if uploaded != data_len {
            return Err(format!(
                "{} is {} bytes in S3, and {} bytes were uploaded",
                data_key, uploaded, data_len
            ));
        }

        for (path, file_name) in [
            (key_index_path, key_index_file_name(hour)),
            (index_path, index_file_name(hour)),
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

        self.indexes.lock().unwrap().remove(&hour);

        Ok(())
    }

    /// Deletes every hour up to `cutoff`: its index first, so an hour deleted halfway is
    /// not read
    pub async fn delete_up_to(&self, cutoff: DateHourKey) -> Result<Vec<DateHourKey>, String> {
        let mut hours = BTreeSet::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let page = self
                .client
                .list_objects_v2(
                    &self.bucket,
                    S3ListObjectsRequest {
                        prefix: (!self.prefix.is_empty()).then_some(self.prefix.as_str()),
                        continuation_token: continuation_token.as_deref(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|err| format!("Listing of {}: {}", self.bucket, err))?;

            for object in &page.objects {
                let hour = object
                    .key
                    .strip_prefix(self.prefix.as_str())
                    .and_then(|name| {
                        name.strip_suffix(".data")
                            .or_else(|| name.strip_suffix(".index"))
                            .or_else(|| name.strip_suffix(".index-by-key"))
                    })
                    .and_then(DateHourKey::parse_from_str);

                if let Some(hour) = hour {
                    if hour <= cutoff {
                        hours.insert(hour);
                    }
                }
            }

            continuation_token = page.next_continuation_token;

            if continuation_token.is_none() {
                break;
            }
        }

        for hour in &hours {
            for key in [
                self.key(index_file_name(*hour)),
                self.key(key_index_file_name(*hour)),
                self.key(data_file_name(*hour)),
            ] {
                self.client
                    .delete_file(&self.bucket, &key)
                    .await
                    .map_err(|err| format!("Delete of {}: {}", key, err))?;
            }

            self.indexes.lock().unwrap().remove(hour);
        }

        Ok(hours.into_iter().collect())
    }
}
