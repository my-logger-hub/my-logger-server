use std::collections::BTreeMap;
use std::time::Duration;

use super::server::GrpcService;
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
        let _request = request.into_inner();

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

        let range = if request.to_time == -1 {
            if request.from_time < 0 {
                let mut from_date = DateTimeAsMicroseconds::now();
                from_date.add_minutes(request.from_time);
                let key = DateHourKey::from(from_date);
                RequestType::HourKey(key)
            } else {
                let key = DateHourKey::from(request.from_time);
                RequestType::HourKey(key)
            }
        } else {
            let from_date: DateTimeAsMicroseconds = request.from_time.into();
            let to_date: DateTimeAsMicroseconds = request.to_time.into();
            RequestType::DateRange(from_date, to_date)
        };

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

    async fn ping(&self, _: tonic::Request<()>) -> Result<tonic::Response<()>, tonic::Status> {
        Ok(tonic::Response::new(()))
    }
}

#[derive(Debug)]
pub enum RequestType {
    HourKey(DateHourKey),
    DateRange(DateTimeAsMicroseconds, DateTimeAsMicroseconds),
}
