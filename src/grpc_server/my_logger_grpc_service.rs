use std::collections::BTreeMap;
use std::time::Duration;

use super::server::GrpcService;
use crate::my_logger_grpc::my_logger_server::MyLogger;
use crate::my_logger_grpc::*;

use my_grpc_extensions::server::generate_server_stream;
use rust_extensions::date_time::DateTimeAsMicroseconds;

const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[tonic::async_trait]
impl MyLogger for GrpcService {
    async fn write(
        &self,
        request: tonic::Request<tonic::Streaming<LogEventGrpcModel>>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let items = my_grpc_extensions::read_grpc_stream::as_vec_with_transformation(
            request.into_inner(),
            READ_TIMEOUT,
            &|grpc_model| grpc_model.into(),
        )
        .await
        .unwrap();

        if let Some(items) = items {
            let log_events = self
                .app
                .settings_reader
                .filter_events(items, |event, filter_events| {
                    for filter in filter_events {
                        if filter.matches_ignore_filter(event) {
                            return false;
                        }
                    }

                    true
                })
                .await;

            self.app.logs_queue.add(log_events).await;
        }

        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"ReadStream", item_name:"LogEventGrpcModel");
    //#[with_result_as_stream("LogEventGrpcModel")]
    async fn read(
        &self,
        request: tonic::Request<ReadLogEventRequest>,
    ) -> Result<tonic::Response<Self::ReadStream>, tonic::Status> {
        let request = request.into_inner();

        let log_levels = if request.levels.len() > 0 {
            Some(
                request
                    .levels()
                    .into_iter()
                    .map(|level| level.into())
                    .collect(),
            )
        } else {
            None
        };

        let context = if request.context_keys.len() > 0 {
            let mut ctx = BTreeMap::new();
            for itm in request.context_keys {
                ctx.insert(itm.key, itm.value);
            }
            Some(ctx)
        } else {
            None
        };

        let from_date = DateTimeAsMicroseconds::new(request.from_time);

        let to_date = if request.to_time > 0 {
            Some(DateTimeAsMicroseconds::new(request.to_time))
        } else {
            None
        };

        let response = self
            .app
            .logs_repo
            .get(
                &request.tenant_id,
                from_date,
                to_date,
                log_levels,
                context,
                request.take as usize,
            )
            .await;

        my_grpc_extensions::grpc_server::send_vec_to_stream(response.into_iter(), |dto| dto.into())
            .await
    }

    async fn get_statistic(
        &self,
        request: tonic::Request<GetStatisticsRequest>,
    ) -> Result<tonic::Response<StatisticData>, tonic::Status> {
        let request = request.into_inner();

        let from_date = DateTimeAsMicroseconds::new(request.from_time);

        let to_date = if request.to_time > 0 {
            Some(DateTimeAsMicroseconds::new(request.to_time))
        } else {
            None
        };

        let response = self
            .app
            .logs_repo
            .get_statistics(&request.tenant_id, from_date, to_date)
            .await;

        let mut result = StatisticData {
            info_count: 0,
            warning_count: 0,
            error_count: 0,
            fatal_count: 0,
            debug_count: 0,
        };

        for itm in response {
            match itm.level {
                crate::repo::dto::LogLevelDto::Info => result.info_count = itm.count.get_value(),
                crate::repo::dto::LogLevelDto::Warning => {
                    result.warning_count = itm.count.get_value()
                }
                crate::repo::dto::LogLevelDto::Error => result.error_count = itm.count.get_value(),
                crate::repo::dto::LogLevelDto::FatalError => {
                    result.fatal_count = itm.count.get_value()
                }
                crate::repo::dto::LogLevelDto::Debug => result.debug_count = itm.count.get_value(),
            }
        }

        Ok(tonic::Response::new(result))
    }

    async fn ping(&self, _: tonic::Request<()>) -> Result<tonic::Response<()>, tonic::Status> {
        Ok(tonic::Response::new(()))
    }
}
