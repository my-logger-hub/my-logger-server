use std::collections::BTreeMap;
use std::time::Duration;

use super::server::GrpcService;
use crate::app::APP_VERSION;
use crate::my_logger_grpc::my_logger_server::MyLogger;
use crate::my_logger_grpc::*;
use crate::repo::dto::IgnoreWhereModel;
use crate::repo::DateHourKey;

use my_grpc_extensions::server::generate_server_stream;
use my_grpc_extensions::server_stream_result::GrpcServerStreamResult;
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

        if is_valid_url_to_update(request.ui_url.as_str()) {
            self.app.update_ui_url(request.ui_url.as_str()).await;
        }

        let levels: Vec<_> = request.levels().collect();

        let response = if request.to_time == 0 {
            let date_key = if request.from_time < 255 {
                let mut now = DateTimeAsMicroseconds::now();
                now.add_hours(request.from_time);
                let date_key: DateHourKey = now.into();
                date_key
            } else {
                let date_key: DateHourKey = request.from_time.into();
                date_key
            };

            let levels = if levels.len() > 0 {
                Some(levels.into_iter().map(|level| level.into()).collect())
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

            println!("ReadLogEventRequest at hour: {:?}", date_key);

            self.app
                .logs_repo
                .get_from_certain_hour(date_key, levels, context, request.take as usize)
                .await
        } else {
            let from_date = DateTimeAsMicroseconds::new(request.from_time);

            let to_date = if request.to_time > 0 {
                Some(DateTimeAsMicroseconds::new(request.to_time))
            } else {
                None
            };

            println!(
                "ReadLogEventRequest at '{}'-'{}'",
                from_date.to_rfc3339(),
                if let Some(to_date) = to_date {
                    to_date.to_rfc3339()
                } else {
                    "None".to_string()
                }
            );
            crate::flows::get_events(
                &self.app,
                levels,
                request.context_keys,
                from_date,
                to_date,
                request.take as usize,
            )
            .await
        };

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

        let response = self.app.logs_repo.get_statistics().await;

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

        println!("ScanAndSearchRequest: {:?}", request);

        if is_valid_url_to_update(request.ui_url.as_str()) {
            self.app.update_ui_url(request.ui_url.as_str()).await;
        }

        let range = RequestType::from(request.from_time, request.to_time);

        println!("ScanAndSearchRequest in range: {:?}", range);

        let response = match range {
            RequestType::HourKey(date_hour_key) => {
                self.app
                    .logs_repo
                    .scan_from_exact_hour(date_hour_key, &request.phrase, request.take as usize)
                    .await
            }
            RequestType::DateRange(from_date, to_date) => {
                crate::flows::search_and_scan(
                    &self.app,
                    from_date,
                    to_date,
                    &request.phrase,
                    request.take as usize,
                )
                .await
            }
        };

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

        crate::flows::ignore_single_event::add(&self.app, request).await;
        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"GetIgnoreSingleEventsStream", item_name:"IgnoreSingleEventGrpcModel");
    async fn get_ignore_single_events(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::GetIgnoreSingleEventsStream>, tonic::Status> {
        let result = crate::flows::ignore_single_event::get_all(&self.app).await;

        my_grpc_extensions::grpc_server::send_vec_to_stream(result.into_iter(), |dto| dto.into())
            .await
    }

    async fn delete_ignore_single_event(
        &self,
        request: tonic::Request<DeleteIgnoreSingleEventGrpcRequest>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let request = request.into_inner();

        crate::flows::ignore_single_event::delete(&self.app, request.id).await;
        return Ok(tonic::Response::new(()));
    }

    generate_server_stream!(stream_name:"GetHourlyStatisticsStream", item_name:"HourlyStatisticsGrpcModel");

    async fn get_hourly_statistics(
        &self,
        request: tonic::Request<GetHourlyStatisticsRequest>,
    ) -> Result<tonic::Response<Self::GetHourlyStatisticsStream>, tonic::Status> {
        let request = request.into_inner();

        let (mut stream_result, result) = GrpcServerStreamResult::new();

        let app = self.app.clone();
        tokio::spawn(async move {
            let result = {
                let read_access = app.hourly_statistics.lock().await;
                read_access.get_max_hours(request.amount_of_hours as usize)
            };

            for (hour, items) in result {
                for (app, statistics) in items {
                    stream_result
                        .send(HourlyStatisticsGrpcModel {
                            hour_key: hour.get_value(),
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

        let keys = self.app.insights_repo.get_keys().await;

        let result = GetInsightsKeysResponse { keys };

        Ok(tonic::Response::new(result))
    }

    async fn get_insights_values(
        &self,
        request: tonic::Request<GetInsightsValuesRequest>,
    ) -> Result<tonic::Response<GetInsightsValuesResponse>, tonic::Status> {
        let request = request.into_inner();

        let values = self
            .app
            .insights_repo
            .get_values(request.key.as_str(), request.phrase.as_str(), 20)
            .await;

        let result = GetInsightsValuesResponse { values };

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

#[derive(Debug)]
pub enum RequestType {
    HourKey(DateHourKey),
    DateRange(DateTimeAsMicroseconds, DateTimeAsMicroseconds),
}

impl RequestType {
    pub fn from(from_time: i64, to_time: i64) -> Self {
        if to_time == 0 {
            let date_key = if from_time < 255 {
                let mut now = DateTimeAsMicroseconds::now();
                now.add_hours(from_time);
                let date_key: DateHourKey = now.into();
                date_key
            } else {
                let date_key: DateHourKey = from_time.into();
                date_key
            };
            return Self::HourKey(date_key);
        }

        Self::DateRange(from_time.into(), to_time.into())
    }
}

pub fn is_valid_url_to_update(url: &str) -> bool {
    url.starts_with("https")
}
