mod post_logs_action;
pub use post_logs_action::*;
mod post_logs_v2_action;
pub use post_logs_v2_action::*;
mod contracts;

use my_http_server::{HttpContext, HttpRequestHeaders};

/// The header naming the product scope of the records: they go by the product scopes
/// without it
const SCOPE_HEADER: &str = "scope";

fn get_scope(ctx: &HttpContext) -> Option<&str> {
    ctx.request
        .get_headers()
        .try_get_case_insensitive_as_str(SCOPE_HEADER)
        .ok()
        .flatten()
}
