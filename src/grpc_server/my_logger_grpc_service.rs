use std::time::Duration;

use super::server::GrpcService;
use crate::my_logger_grpc::my_logger_server::MyLogger;
use crate::my_logger_grpc::*;
use crate::repo::ignore_events::IgnoreEventModel;
use crate::{app::APP_VERSION, repo::logs::LogEventCtxFileGrpcModel};

use my_grpc_extensions::server::generate_server_stream;
use my_grpc_extensions::server_stream_result::GrpcServerStreamResult;
use rust_extensions::date_time::{DateTimeAsMicroseconds, HourKey, IntervalKey};

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

        if is_valid_url_to_update(request.ui_url.as_str()) {
            self.app.update_ui_url(request.ui_url.as_str()).await;
        }

        let to_date = if request.to_time > 0 {
            Some(DateTimeAsMicroseconds::new(request.to_time))
        } else {
            None
        };

        let levels: Vec<_> = request.levels().map(|itm| itm.into()).collect();

        let ctx: Vec<_> = request
            .context_keys
            .into_iter()
            .map(|itm| LogEventCtxFileGrpcModel {
                key: itm.key,
                value: itm.value,
            })
            .collect();

        let response = crate::flows::get_events(
            &self.app,
            levels,
            ctx,
            DateTimeAsMicroseconds::new(request.from_time),
            to_date,
            request.take as usize,
        )
        .await;

        my_grpc_extensions::grpc_server::send_vec_to_stream(response.into_iter(), move |dto| {
            super::mapper::to_log_event_grpc_model(dto)
        })
        .await
    }

    async fn get_statistic(
        &self,
        request: tonic::Request<GetStatisticsRequest>,
    ) -> Result<tonic::Response<StatisticData>, tonic::Status> {
        let request = request.into_inner();

        if is_valid_url_to_update(request.ui_url.as_str()) {
            self.app.update_ui_url(request.ui_url.as_str()).await;
        }

        let from: IntervalKey<HourKey> = DateTimeAsMicroseconds::new(request.from_time).into();
        let to: IntervalKey<HourKey> = DateTimeAsMicroseconds::new(request.to_time).into();

        let response = self.app.hour_statistics_repo.get(from, to).await;

        let mut result = StatisticData {
            info_count: 0,
            warning_count: 0,
            error_count: 0,
            fatal_count: 0,
            debug_count: 0,
        };

        for itm in response {
            for data in itm.1.values() {
                result.debug_count += data.debug as i32;
                result.error_count += data.error as i32;
                result.fatal_count += data.fatal_error as i32;
                result.info_count += data.info as i32;
                result.warning_count += data.warning as i32;
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

        println!("ScanAndSearchRequest: {:?}", request);

        if is_valid_url_to_update(request.ui_url.as_str()) {
            self.app.update_ui_url(request.ui_url.as_str()).await;
        }

        let from_date = DateTimeAsMicroseconds::new(request.from_time);
        let to_date = DateTimeAsMicroseconds::new(request.to_time);

        let response = crate::flows::search_and_scan(
            &self.app,
            from_date,
            to_date,
            &request.phrase,
            request.take as usize,
        )
        .await;

        my_grpc_extensions::grpc_server::send_vec_to_stream(response.into_iter(), move |dto| {
            super::mapper::to_log_event_grpc_model(dto)
        })
        .await
    }

    async fn set_ignore_event(
        &self,
        request: tonic::Request<IgnoreEventGrpcModel>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();

        crate::flows::ignore_event::add(&self.app, request.into()).await;
        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"GetIgnoreEventsStream", item_name:"IgnoreEventGrpcModel");

    async fn get_ignore_events(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::GetIgnoreEventsStream>, tonic::Status> {
        let response = self.app.ignore_events_repo.get_all().await;

        let response: Vec<IgnoreEventGrpcModel> =
            response.into_iter().map(|itm| itm.into()).collect();

        my_grpc_extensions::grpc_server::send_vec_to_stream(response.into_iter(), |dto| dto.into())
            .await
    }

    async fn delete_ignore_event(
        &self,
        request: tonic::Request<DeleteIgnoreEventGrpcRequest>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();
        crate::flows::ignore_event::remove(
            &self.app,
            IgnoreEventModel {
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

        crate::flows::ignore_single_event::add(&self.app, request.into()).await;

        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"GetIgnoreSingleEventsStream", item_name:"IgnoreSingleEventGrpcModel");
    async fn get_ignore_single_events(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::GetIgnoreSingleEventsStream>, tonic::Status> {
        let result = self.app.ignore_single_events_repo.get_all().await;

        my_grpc_extensions::grpc_server::send_vec_to_stream(result.into_iter(), |dto| {
            dto.as_ref().into()
        })
        .await
    }

    async fn delete_ignore_single_event(
        &self,
        request: tonic::Request<DeleteIgnoreSingleEventGrpcRequest>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();

        crate::flows::ignore_single_event::delete(&self.app, request.id.as_str()).await;
        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"GetHourlyStatisticsStream", item_name:"HourlyStatisticsGrpcModel");

    async fn get_hourly_statistics(
        &self,
        request: tonic::Request<GetHourlyStatisticsRequest>,
    ) -> Result<tonic::Response<Self::GetHourlyStatisticsStream>, tonic::Status> {
        let request = request.into_inner();

        let response = self
            .app
            .hour_statistics_repo
            .get_highest_and_below(request.amount_of_hours as usize)
            .await;

        let (mut stream_result, result) = GrpcServerStreamResult::new();

        tokio::spawn(async move {
            for (hour, items) in response {
                for (app, statistics) in items {
                    stream_result
                        .send(HourlyStatisticsGrpcModel {
                            hour_key: hour.to_i64() as u64,
                            app,
                            info_count: statistics.info,
                            warning_count: statistics.warning,
                            error_count: statistics.error,
                            fatal_count: statistics.fatal_error,
                            debug_count: statistics.debug,
                        })
                        .await;
                }
            }
        });

        Ok(result)
    }

    async fn get_insights_keys(
        &self,
        request: tonic::Request<()>,
    ) -> Result<tonic::Response<GetInsightsKeysResponse>, tonic::Status> {
        let _request = request.into_inner();

        /*

               let keys = self.app.insights_repo.get_keys().await;

               let result = GetInsightsKeysResponse { keys };
        */
        Ok(tonic::Response::new(GetInsightsKeysResponse {
            keys: vec![],
        }))
    }

    async fn get_insights_values(
        &self,
        request: tonic::Request<GetInsightsValuesRequest>,
    ) -> Result<tonic::Response<GetInsightsValuesResponse>, tonic::Status> {
        let _request = request.into_inner();

        /*
               let values = self
                   .app
                   .insights_repo
                   .get_values(request.key.as_str(), request.phrase.as_str(), 20)
                   .await;
        */
        let result = GetInsightsValuesResponse { values: vec![] };

        Ok(tonic::Response::new(result))
    }

    async fn get_server_info(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<ServerInfoGrpcResponse>, tonic::Status> {
        let response = ServerInfoGrpcResponse {
            version: APP_VERSION.to_string(),
            hours_to_gc: self.app.settings_reader.get_hours_to_gc().await as u32,
        };

        Ok(tonic::Response::new(response))
    }

    async fn ping(&self, _: tonic::Request<()>) -> Result<tonic::Response<()>, tonic::Status> {
        Ok(tonic::Response::new(()))
    }
}

pub fn is_valid_url_to_update(url: &str) -> bool {
    url.starts_with("https")
}
