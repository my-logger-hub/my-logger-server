use std::{
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};

use rust_extensions::{date_time::DateTimeAsMicroseconds, MyTimerTick, RepeatTimerIteration};

use crate::app::AppContext;

/// The hours of S3 older than `hours_to_gc` of `settings.json` are looked for this often
const S3_GC_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Uploads the Warning, Error and FatalError hours whose time on the local disk is over,
/// and deletes the hours of S3 older than `hours_to_gc`
pub struct UploadToS3Timer {
    app: Arc<AppContext>,
    last_s3_gc: AtomicI64,
}

impl UploadToS3Timer {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self {
            app,
            last_s3_gc: AtomicI64::new(0),
        }
    }

    async fn report(&self, message: String) {
        println!("{}", message);
        self.app.debug_console.push(message).await;
    }
}

#[async_trait::async_trait]
impl MyTimerTick for UploadToS3Timer {
    async fn tick(&self) -> RepeatTimerIteration {
        let now = DateTimeAsMicroseconds::now();
        let hours_to_gc = self.app.settings_repo.get_hours_to_gc().await;

        for (scope, hour) in self.app.logs_storage.hours_to_upload(now, hours_to_gc) {
            let name = format!("{}/{}", scope, hour.get_value());

            let message = match self.app.logs_storage.clone().upload_hour(scope, hour).await {
                Ok(()) => format!("S3: {} is uploaded", name),
                Err(err) => format!("S3: {} is not uploaded: {}", name, err),
            };

            self.report(message).await;
        }

        let last_s3_gc = self.last_s3_gc.load(Ordering::Relaxed);

        if now.unix_microseconds - last_s3_gc >= S3_GC_INTERVAL.as_micros() as i64 {
            self.last_s3_gc.store(now.unix_microseconds, Ordering::Relaxed);

            match self.app.logs_storage.gc_s3(now, hours_to_gc).await {
                Ok(hours) => {
                    for (scope, hour) in hours {
                        self.report(format!("S3 GC: {}/{} is deleted", scope, hour.get_value()))
                            .await;
                    }
                }
                Err(err) => self.report(format!("S3 GC failed: {}", err)).await,
            }
        }

        RepeatTimerIteration::WithInterval
    }
}
