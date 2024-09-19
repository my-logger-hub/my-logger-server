use crate::app::AppContext;

pub async fn delete(app: &AppContext, id: String) {
    let mut write_access = app.ignore_single_event_cache.lock().await;

    if !write_access.initialized {
        let items = super::persistence::get_all(app).await;
        write_access.init(items.clone());
    }

    write_access.delete(&id);

    super::persistence::save(app).await;
}
