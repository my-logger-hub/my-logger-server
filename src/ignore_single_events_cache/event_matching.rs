use crate::{log_item::LogEvent, my_logger_grpc::IgnoreSingleEventGrpcModel};

pub fn match_event(itm: &LogEvent, ignore_single_event: &IgnoreSingleEventGrpcModel) -> bool {
    let level_as_i32 = level_is_i32(itm.level);
    let matched = ignore_single_event
        .levels
        .iter()
        .any(|level| *level == level_as_i32);

    if !matched {
        return false;
    }

    if !itm.message.contains(&ignore_single_event.message_match) {
        return false;
    }

    for da_ctx in &ignore_single_event.context_match {
        match itm.ctx.get(&da_ctx.key) {
            Some(value) => {
                if !value.contains(&da_ctx.value) {
                    return false;
                }
            }
            None => {
                return false;
            }
        }
    }

    true
}

fn level_is_i32(level: my_logger::LogLevel) -> i32 {
    match level {
        my_logger::LogLevel::Info => 0,
        my_logger::LogLevel::Warning => 1,
        my_logger::LogLevel::Error => 2,
        my_logger::LogLevel::FatalError => 3,
        my_logger::LogLevel::Debug => 4,
    }
}
