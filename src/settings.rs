use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TelegramSettings {
    pub api_key: String,
    pub chat_id: i64,
    pub message_thread_id: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ElasticSettings {
    pub esecure: String,
    pub url: String,
    pub env_source: String,
}

/// Where the Warning, Error and FatalError hours go 2 hours after they end, to be kept for
/// `hours_to_gc` of `settings.json`. Without it they are deleted then.
///
/// One connection string, the `Key=Value;Key=Value` shape of my-service-bus-persistence:
///
/// ```text
/// s3_conn_string: "Endpoint=https://fsn1.your-objectstorage.com;Region=fsn1;AccessKey=...;SecretKey=...;Bucket=my-logs"
/// ```
#[derive(Debug, Clone)]
pub struct S3ConnectionSettings {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    /// `Prefix=logs` - the folder of the hours in the bucket; `logs` when it is not given
    pub prefix: Option<String>,
    /// `Debug=1` - trace every S3 request to the console. Off unless asked for.
    pub debug: bool,
}

impl S3ConnectionSettings {
    /// A missing or misspelled key fails the start rather than silently leaving S3 out
    pub fn parse(conn_string: &str) -> Self {
        let mut endpoint = None;
        let mut region = None;
        let mut access_key = None;
        let mut secret_key = None;
        let mut bucket = None;
        let mut prefix = None;
        let mut debug = None;

        for pair in conn_string.split(';') {
            let pair = pair.trim();

            if pair.is_empty() {
                continue;
            }

            // Only the first `=` separates - a base64 secret key carries its own
            let Some(separator) = pair.find('=') else {
                panic!("Invalid s3_conn_string: '{}' is not Key=Value", pair);
            };

            let key = pair[..separator].trim();
            let value = pair[separator + 1..].trim().to_string();

            match key {
                "Endpoint" => endpoint = Some(value),
                "Region" => region = Some(value),
                "AccessKey" => access_key = Some(value),
                "SecretKey" => secret_key = Some(value),
                "Bucket" => bucket = Some(value),
                "Prefix" => prefix = Some(value),
                "Debug" => debug = Some(parse_bool(value.as_str(), "Debug")),
                _ => panic!(
                    "Invalid s3_conn_string: unknown key '{}'. Expected Endpoint, Region, AccessKey, SecretKey, Bucket, and optionally Prefix and Debug",
                    key
                ),
            }
        }

        Self {
            endpoint: required(endpoint, "Endpoint"),
            region: required(region, "Region"),
            access_key: required(access_key, "AccessKey"),
            secret_key: required(secret_key, "SecretKey"),
            bucket: required(bucket, "Bucket"),
            prefix,
            debug: debug.unwrap_or(false),
        }
    }
}

/// Spelled out rather than `== "1"`: a `Debug=true` silently meaning "off" is worse than a
/// refusal to start
fn parse_bool(value: &str, key: &str) -> bool {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => panic!(
            "Invalid s3_conn_string: '{}' expects 1/0, true/false, yes/no or on/off - got '{}'",
            key, value
        ),
    }
}

fn required(value: Option<String>, key: &str) -> String {
    match value {
        Some(value) if !value.is_empty() => value,
        _ => panic!("Invalid s3_conn_string: '{}' is missing", key),
    }
}

#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    pub logs_db_path: String,

    pub telegram_settings: Option<TelegramSettings>,


    pub elastic: Option<ElasticSettings>,


    pub s3_conn_string: Option<String>,

    pub env_name: String,
}

impl SettingsReader {
    pub async fn get_telegram_settings(&self) -> Option<TelegramSettings> {
        let read_access = self.settings.read().await;
        read_access.telegram_settings.clone()
    }

    pub async fn get_env_name(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.env_name.clone()
    }

    pub async fn get_elastic_settings(&self) -> Option<ElasticSettings> {
        let read_access = self.settings.read().await;
        read_access.elastic.clone()
    }

    /// `None` - no S3: the hours are deleted 2 hours after they end
    pub async fn get_s3_connection(&self) -> Option<S3ConnectionSettings> {
        let read_access = self.settings.read().await;
        let conn_string = read_access.s3_conn_string.as_deref()?.trim();

        if conn_string.is_empty() {
            return None;
        }

        Some(S3ConnectionSettings::parse(conn_string))
    }

    pub async fn get_logs_db_path(&self, file_name: Option<&str>) -> String {
        let read_access = self.settings.read().await;

        let mut result = if read_access.logs_db_path.starts_with("~") {
            read_access
                .logs_db_path
                .replace("~", &std::env::var("HOME").unwrap())
        } else {
            read_access.logs_db_path.clone()
        };

        if !result.ends_with(std::path::MAIN_SEPARATOR) {
            result.push('/')
        }

        if let Some(file_name) = file_name {
            result.push_str(file_name);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_connection_string() {
        let parsed = S3ConnectionSettings::parse(
            "Endpoint=https://s3.eu-central-1.amazonaws.com;Region=eu-central-1;AccessKey=AKIA123;SecretKey=abc/def+ghi=;Bucket=my-bucket",
        );

        assert_eq!("https://s3.eu-central-1.amazonaws.com", parsed.endpoint);
        assert_eq!("eu-central-1", parsed.region);
        assert_eq!("AKIA123", parsed.access_key);
        // A base64 secret carries its own '=' - only the first one separates
        assert_eq!("abc/def+ghi=", parsed.secret_key);
        assert_eq!("my-bucket", parsed.bucket);
        assert_eq!(None, parsed.prefix);
        assert!(!parsed.debug);
    }

    #[test]
    fn prefix_and_debug_are_optional() {
        let parsed = S3ConnectionSettings::parse(
            "Endpoint=https://s3;Region=eu;AccessKey=a;SecretKey=b;Bucket=c;Prefix=my-logs;Debug=1;",
        );

        assert_eq!(Some("my-logs"), parsed.prefix.as_deref());
        assert!(parsed.debug);

        for value in ["0", "false", "no", "off"] {
            let parsed = S3ConnectionSettings::parse(&format!(
                "Endpoint=https://s3;Region=eu;AccessKey=a;SecretKey=b;Bucket=c;Debug={}",
                value
            ));
            assert!(!parsed.debug, "'{}' should have left debug off", value);
        }
    }

    #[test]
    #[should_panic(expected = "'Bucket' is missing")]
    fn a_missing_key_is_loud() {
        S3ConnectionSettings::parse("Endpoint=https://s3;Region=eu;AccessKey=a;SecretKey=b");
    }

    #[test]
    #[should_panic(expected = "unknown key")]
    fn a_typo_is_loud() {
        S3ConnectionSettings::parse(
            "Endpoint=https://s3;Region=eu;AccessKey=a;SecretKey=b;Buckett=c",
        );
    }

    #[test]
    #[should_panic(expected = "'Debug' expects 1/0")]
    fn an_unreadable_debug_value_is_loud() {
        S3ConnectionSettings::parse(
            "Endpoint=https://s3;Region=eu;AccessKey=a;SecretKey=b;Bucket=c;Debug=maybe",
        );
    }
}
