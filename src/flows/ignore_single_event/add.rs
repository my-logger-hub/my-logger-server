use crate::{app::AppContext, my_logger_grpc::*};

pub async fn add(app: &AppContext, item: IgnoreSingleEventGrpcModel) {
    app.ignore_single_event_cache.lock().await.add(item);
    super::persistence::save(app).await;
}
