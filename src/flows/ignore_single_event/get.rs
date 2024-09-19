use crate::{app::AppContext, my_logger_grpc::*};
pub async fn get_all(app: &AppContext) -> Vec<IgnoreSingleEventGrpcModel> {
    let mut write_access = app.ignore_single_event_cache.lock().await;

    if !write_access.initialized {
        let items = super::persistence::get_all(app).await;
        write_access.init(items.clone());
    }

    write_access.get_all()
}
