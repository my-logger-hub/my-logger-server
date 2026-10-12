use std::collections::BTreeMap;
use std::time::Duration;

use super::server::GrpcService;
use crate::app::APP_VERSION;
use crate::hourly_statistics::StatisticsHour;
use crate::logs_storage::SearchDirection;
use crate::my_logger_grpc::my_logger_server::MyLogger;
use crate::my_logger_grpc::*;
use crate::repo::dto::{IgnoreWhereModel, LogItemDto, LogLevelDto};
use crate::repo::DateHourKey;

use my_grpc_extensions::server::generate_server_stream;
use my_grpc_extensions::server_stream_result::GrpcServerStreamResult;
use rust_extensions::date_time::DateTimeAsMicroseconds;

const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The metadata of `Write` naming the product scope of the records
const SCOPE_HEADER: &str = "scope";

#[tonic::async_trait]
impl MyLogger for GrpcService {
    async fn write(
        &self,
        request: tonic::Request<tonic::Streaming<LogEventGrpcModel>>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        // The product scope the writer names: the records go by the product scopes without it
        let scope = request
            .metadata()
            .get(SCOPE_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let items = my_grpc_extensions::read_grpc_stream::as_vec_with_transformation(
            request.into_inner(),
            READ_TIMEOUT,
            &|grpc_model| grpc_model.into(),
        )
        .await
        .unwrap();

        if items.len() > 0 {
            crate::flows::post_items(&self.app, items, scope.as_deref()).await;
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

        let product_scope = self.requested_scope(request.product_scope.as_deref())?;
        let levels: Vec<_> = request.levels().collect();
        let range = RequestType::from_request(request.from_time, request.to_time);
        println!("ReadLogEventRequest in range: {:?}", range);

        let response = match range {
            RequestType::HourKey(date_key) => {
                let log_levels = if levels.len() > 0 {
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

                crate::flows::search_logs(
                    &self.app,
                    &product_scope,
                    date_key.hour_start(),
                    date_key.hour_end(),
                    log_levels,
                    context,
                    None,
                    request.take as usize,
                    SearchDirection::Down,
                )
                .await
            }
            RequestType::DateRange(from_date, to_date) => {
                crate::flows::get_events(
                    &self.app,
                    &product_scope,
                    levels,
                    request.context_keys,
                    from_date,
                    Some(to_date),
                    request.take as usize,
                )
                .await
            }
        };

        my_grpc_extensions::grpc_server_streams::send_from_iterator_with_transformation(
            response.into_iter(),
            move |dto| super::mapper::to_log_event_grpc_model(dto),
        )
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

        let hour: StatisticsHour = DateTimeAsMicroseconds::now().into();
        let totals = self
            .app
            .hourly_statistics
            .lock()
            .await
            .get_hour_totals(hour);

        Ok(tonic::Response::new(StatisticData {
            info_count: totals.info as i32,
            warning_count: totals.warning as i32,
            error_count: totals.error as i32,
            fatal_count: totals.fatal_error as i32,
            debug_count: totals.debug as i32,
        }))
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

        let product_scope = self.requested_scope(request.product_scope.as_deref())?;
        let range = RequestType::from_request(request.from_time, request.to_time);

        println!("ScanAndSearchRequest in range: {:?}", range);

        let response = match range {
            RequestType::HourKey(date_hour_key) => {
                crate::flows::search_logs(
                    &self.app,
                    &product_scope,
                    date_hour_key.hour_start(),
                    date_hour_key.hour_end(),
                    None,
                    None,
                    Some(request.phrase.as_str()),
                    request.take as usize,
                    SearchDirection::Down,
                )
                .await
            }
            RequestType::DateRange(from_date, to_date) => {
                crate::flows::search_and_scan(
                    &self.app,
                    &product_scope,
                    from_date,
                    to_date,
                    &request.phrase,
                    request.take as usize,
                )
                .await
            }
        };

        my_grpc_extensions::grpc_server_streams::send_from_iterator_with_transformation(
            response.into_iter(),
            move |dto| super::mapper::to_log_event_grpc_model(dto),
        )
        .await
    }

    generate_server_stream!(stream_name:"SearchUpStream", item_name:"LogEventGrpcModel");
    async fn search_up(
        &self,
        request: tonic::Request<SearchLogsRequest>,
    ) -> Result<tonic::Response<Self::SearchUpStream>, tonic::Status> {
        let response = self
            .search_logs(request.into_inner(), SearchDirection::Up)
            .await?;

        my_grpc_extensions::grpc_server_streams::send_from_iterator_with_transformation(
            response.into_iter(),
            move |dto| super::mapper::to_log_event_grpc_model(dto),
        )
        .await
    }

    generate_server_stream!(stream_name:"SearchDownStream", item_name:"LogEventGrpcModel");
    async fn search_down(
        &self,
        request: tonic::Request<SearchLogsRequest>,
    ) -> Result<tonic::Response<Self::SearchDownStream>, tonic::Status> {
        let response = self
            .search_logs(request.into_inner(), SearchDirection::Down)
            .await?;

        my_grpc_extensions::grpc_server_streams::send_from_iterator_with_transformation(
            response.into_iter(),
            move |dto| super::mapper::to_log_event_grpc_model(dto),
        )
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
        my_grpc_extensions::grpc_server_streams::send_from_iterator_with_transformation(response.into_iter(), |dto| {
            dto.into()
        })
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

        my_grpc_extensions::grpc_server_streams::send_from_iterator_with_transformation(result.into_iter(), |dto| {
            dto.into()
        })
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

            for (hour, scopes) in result {
                for (scope, apps) in scopes {
                    for (app, statistics) in apps {
                        let _ = stream_result
                            .send(HourlyStatisticsGrpcModel {
                                hour_key: hour.get_value(),
                                app,
                                info_count: statistics.info,
                                warning_count: statistics.warning,
                                error_count: statistics.error,
                                fatal_count: statistics.fatal_error,
                                debug_count: statistics.debug,
                                scope: scope.clone(),
                            })
                            .await;
                    }
                }
            }
        });

        Ok(result)
    }

    /// No insights are collected for now: the UI gets no keys
    async fn get_insights_keys(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<GetInsightsKeysResponse>, tonic::Status> {
        Ok(tonic::Response::new(GetInsightsKeysResponse { keys: Vec::new() }))
    }

    async fn get_insights_values(
        &self,
        _request: tonic::Request<GetInsightsValuesRequest>,
    ) -> Result<tonic::Response<GetInsightsValuesResponse>, tonic::Status> {
        Ok(tonic::Response::new(GetInsightsValuesResponse { values: Vec::new() }))
    }

    async fn get_server_info(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<ServerInfoGrpcResponse>, tonic::Status> {
        let response = ServerInfoGrpcResponse {
            version: APP_VERSION.to_string(),
            hours_to_gc: self.app.settings_repo.get_hours_to_gc().await as u32,
        };

        Ok(tonic::Response::new(response))
    }

    async fn get_product_scopes(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<GetProductScopesResponse>, tonic::Status> {
        let product_scopes = self.app.logs_storage.product_scopes();
        let with_records = self.app.logs_storage.scopes_with_records();

        let scopes = product_scopes
            .list(&with_records)
            .into_iter()
            .map(|(name, applications)| ProductScopeGrpcModel {
                name: name.to_string(),
                applications: applications.to_vec(),
            })
            .collect();

        Ok(tonic::Response::new(GetProductScopesResponse {
            default_scope: product_scopes.default_scope().to_string(),
            scopes,
        }))
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

impl GrpcService {
    async fn search_logs(
        &self,
        request: SearchLogsRequest,
        direction: SearchDirection,
    ) -> Result<Vec<LogItemDto>, tonic::Status> {
        if is_valid_url_to_update(request.ui_url.as_str()) {
            self.app.update_ui_url(request.ui_url.as_str()).await;
        }

        let product_scope = self.requested_scope(request.product_scope.as_deref())?;

        let levels: Vec<LogLevelDto> = request.levels().map(|level| level.into()).collect();
        let context: BTreeMap<String, String> = request
            .context_keys
            .into_iter()
            .map(|ctx| (ctx.key, ctx.value))
            .collect();

        let (from_date, to_date) =
            RequestType::from_request(request.from_time, request.to_time).get_range();

        let result = crate::flows::search_logs(
            &self.app,
            &product_scope,
            from_date,
            to_date,
            (!levels.is_empty()).then_some(levels),
            (!context.is_empty()).then_some(context),
            Some(request.phrase.as_str()),
            request.take as usize,
            direction,
        )
        .await;

        Ok(result)
    }

    /// The default product scope when the request names none
    fn requested_scope(&self, scope: Option<&str>) -> Result<String, tonic::Status> {
        self.app
            .logs_storage
            .product_scopes()
            .requested(scope)
            .map_err(tonic::Status::invalid_argument)
    }
}

impl RequestType {
    /// The first and the last microsecond asked for
    pub fn get_range(&self) -> (DateTimeAsMicroseconds, DateTimeAsMicroseconds) {
        match self {
            RequestType::HourKey(date_key) => (
                date_key.hour_start(),
                DateTimeAsMicroseconds::new(date_key.hour_end().unix_microseconds - 1),
            ),
            RequestType::DateRange(from, to) => (*from, *to),
        }
    }

    pub fn from_request(from_time: i64, to_time: i64) -> Self {
        if to_time == 0 {
            let date_key = if from_time <= 0 {
                let mut now = DateTimeAsMicroseconds::now();
                now.add_hours(from_time);
                let date_key: DateHourKey = now.into();
                date_key
            } else {
                let date_key: DateHourKey = from_time.into();
                date_key
            };

            // Zero hours ago is not the current hour alone: every hour there is, up to its end
            if from_time == 0 {
                let (_, to) = Self::HourKey(date_key).get_range();
                return Self::DateRange(DateTimeAsMicroseconds::new(0), to);
            }

            return Self::HourKey(date_key);
        }

        Self::DateRange(from_time.into(), to_time.into())
    }
}

pub fn is_valid_url_to_update(url: &str) -> bool {
    url.starts_with("https")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_hours_ago_is_every_hour_up_to_the_end_of_the_current_one() {
        let now = DateTimeAsMicroseconds::now().unix_microseconds;
        let hour_micros = 3600 * 1_000_000;

        let (from, to) = RequestType::from_request(0, 0).get_range();
        assert_eq!(from.unix_microseconds, 0);
        assert!(to.unix_microseconds >= now && to.unix_microseconds < now + hour_micros);

        // Hours ago, and an hour by its key: that hour alone
        let (from, to) = RequestType::from_request(-1, 0).get_range();
        assert_eq!(to.unix_microseconds - from.unix_microseconds, hour_micros - 1);
        assert!(to.unix_microseconds < now && to.unix_microseconds >= now - hour_micros);

        assert!(matches!(
            RequestType::from_request(2026101112, 0),
            RequestType::HourKey(key) if key.get_value() == 2026101112
        ));
    }
}
