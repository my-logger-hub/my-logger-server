pub fn generate_log_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
