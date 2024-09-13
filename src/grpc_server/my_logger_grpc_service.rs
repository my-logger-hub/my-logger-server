use std::time::Duration;

use super::server::GrpcService;
use crate::my_logger_grpc::my_logger_server::MyLogger;
use crate::my_logger_grpc::*;
use crate::repo::dto::IgnoreWhereModel;

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
            crate::flows::post_items(&self.app, items).await;
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

        let from_date = DateTimeAsMicroseconds::new(request.from_time);

        let to_date = if request.to_time > 0 {
            Some(DateTimeAsMicroseconds::new(request.to_time))
        } else {
            None
        };

        let levels: Vec<_> = request.levels().collect();

        let tenant_id = request.tenant_id;

        let response = crate::flows::get_events(
            &self.app,
            levels,
            request.context_keys,
            from_date,
            to_date,
            tenant_id.as_str(),
            request.take as usize,
        )
        .await;

        my_grpc_extensions::grpc_server::send_vec_to_stream(response.into_iter(), move |dto| {
            super::mapper::to_log_event_grpc_model(tenant_id.to_string(), dto)
        })
        .await
    }

    async fn get_statistic(
        &self,
        request: tonic::Request<GetStatisticsRequest>,
    ) -> Result<tonic::Response<StatisticData>, tonic::Status> {
        let request = request.into_inner();

        /*
               let from_date = DateTimeAsMicroseconds::new(request.from_time);

               let to_date = if request.to_time > 0 {
                   Some(DateTimeAsMicroseconds::new(request.to_time))
               } else {
                   None
               };
        */
        let response = self.app.logs_repo.get_statistics(&request.tenant_id).await;

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

    generate_server_stream!(stream_name:"ScanAndSearchStream", item_name:"LogEventGrpcModel");
    async fn scan_and_search(
        &self,
        request: tonic::Request<ScanAndSearchRequest>,
    ) -> Result<tonic::Response<Self::ScanAndSearchStream>, tonic::Status> {
        let request = request.into_inner();

        let from_date: DateTimeAsMicroseconds = request.from_time.into();
        let to_date: DateTimeAsMicroseconds = request.to_time.into();

        let tenant_id = request.tenant_id;

        let response = crate::flows::search_and_scan(
            &self.app,
            &tenant_id,
            from_date,
            to_date,
            &request.phrase,
            request.take as usize,
        )
        .await;

        my_grpc_extensions::grpc_server::send_vec_to_stream(response.into_iter(), move |dto| {
            super::mapper::to_log_event_grpc_model(tenant_id.to_string(), dto)
        })
        .await
    }

    async fn set_ignore_event(
        &self,
        request: tonic::Request<IgnoreEventGrpcModel>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();
        crate::flows::add_ignore_event(&self.app, request.into()).await;
        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"GetIgnoreEventsStream", item_name:"IgnoreEventGrpcModel");

    async fn get_ignore_events(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::GetIgnoreEventsStream>, tonic::Status> {
        let response = self.app.settings_repo.get_ignore_events().await;
        my_grpc_extensions::grpc_server::send_vec_to_stream(response.into_iter(), |dto| dto.into())
            .await
    }

    async fn delete_ignore_event(
        &self,
        request: tonic::Request<DeleteIgnoreEventGrpcRequest>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();
        crate::flows::remove_ignore_event(
            &self.app,
            IgnoreWhereModel {
                level: request.level().into(),
                application: request.application,
                marker: request.marker,
            },
        )
        .await;
        return Ok(tonic::Response::new(()));
    }

    async fn set_ignore_single_event(
        &self,
        request: tonic::Request<IgnoreSingleEventGrpcModel>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();

        self.app.ignore_single_event_cache.lock().await.add(request);
        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"GetIgnoreSingleEventsStream", item_name:"IgnoreSingleEventGrpcModel");
    async fn get_ignore_single_events(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::GetIgnoreSingleEventsStream>, tonic::Status> {
        let result = self.app.ignore_single_event_cache.lock().await.get_all();

        my_grpc_extensions::grpc_server::send_vec_to_stream(result.into_iter(), |dto| dto.into())
            .await
    }

    async fn delete_ignore_single_event(
        &self,
        request: tonic::Request<DeleteIgnoreSingleEventGrpcRequest>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();
        self.app
            .ignore_single_event_cache
            .lock()
            .await
            .delete(&request.id);
        return Ok(tonic::Response::new(()));
    }

    async fn ping(&self, _: tonic::Request<()>) -> Result<tonic::Response<()>, tonic::Status> {
        Ok(tonic::Response::new(()))
    }
}
