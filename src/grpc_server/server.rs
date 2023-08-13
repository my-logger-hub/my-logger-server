use crate::app::AppContext;
use crate::my_logger_grpc::my_logger_server::MyLoggerServer;

use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;

#[derive(Clone)]

pub struct GrpcService {
    pub app: Arc<AppContext>,
}

impl GrpcService {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

pub fn start(app: Arc<AppContext>) {
    tokio::spawn(start_server(app));
}

async fn start_server(app: Arc<AppContext>) {
    let addr = SocketAddr::from(([0, 0, 0, 0], 8888));

    let service = GrpcService::new(app);

    println!("Listening to {:?} as grpc endpoint", addr);

    anyhow::Context::context(
        Server::builder()
            .add_service(MyLoggerServer::new(service.clone()))
            .serve(addr)
            .await,
        "Server error",
    )
    .unwrap();
}
