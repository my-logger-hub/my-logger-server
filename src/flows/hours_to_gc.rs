use crate::app::AppContext;

/// How long the Warning, Error and FatalError hours are kept in S3 from then on: the hours
/// older are deleted at the next GC of S3. An hour older than that by the time it is to be
/// uploaded - 3 hours after it starts - is not uploaded but deleted.
pub async fn set_hours_to_gc(app: &AppContext, hours_to_gc: i64) -> Result<(), String> {
    if hours_to_gc < 0 {
        return Err(format!("hours_to_gc can not be {}", hours_to_gc));
    }

    app.settings_repo.set_hours_to_gc(hours_to_gc as u64).await;
    Ok(())
}
