use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::dto::*;

/// What `settings.json` holds. An older file is the array of the ignore events alone.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    ignore_events: Vec<IgnoreItemDto>,
    #[serde(default = "default_product_scope")]
    default_product_scope: String,
    /// How long the Warning, Error and FatalError hours are kept in S3
    #[serde(default = "default_hours_to_gc")]
    hours_to_gc: u64,
    #[serde(default)]
    product_scopes: Vec<ProductScopeDto>,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            ignore_events: Vec::new(),
            default_product_scope: default_product_scope(),
            hours_to_gc: default_hours_to_gc(),
            product_scopes: Vec::new(),
        }
    }
}

/// A week, until it is set
const DEFAULT_HOURS_TO_GC: u64 = 7 * 24;

fn default_hours_to_gc() -> u64 {
    DEFAULT_HOURS_TO_GC
}

fn default_product_scope() -> String {
    crate::product_scopes::DEFAULT_PRODUCT_SCOPE.to_string()
}

impl SettingsFile {
    fn parse(bytes: &[u8]) -> Self {
        if let Ok(file) = serde_json::from_slice::<SettingsFile>(bytes) {
            return file;
        }

        Self {
            ignore_events: serde_json::from_slice(bytes).unwrap_or_default(),
            ..Default::default()
        }
    }

    fn product_scopes(&self) -> ProductScopesSettings {
        ProductScopesSettings {
            default_scope: self.default_product_scope.clone(),
            scopes: self.product_scopes.clone(),
        }
    }
}

pub struct SettingsRepo {
    items: Mutex<SettingsFile>,
    path: String,
}

impl SettingsRepo {
    pub async fn new(path: String) -> Self {
        let items = match tokio::fs::read(&path).await {
            Ok(bytes) => SettingsFile::parse(&bytes),
            Err(_) => SettingsFile::default(),
        };
        Self {
            items: Mutex::new(items),
            path,
        }
    }

    pub async fn get_ignore_events(&self) -> Vec<IgnoreItemDto> {
        let mut write_access = self.items.lock().await;
        let now = DateTimeAsMicroseconds::now();
        let before = write_access.ignore_events.len();
        write_access.ignore_events.retain(|item| !item.is_expired(now));
        if write_access.ignore_events.len() != before {
            persist(&self.path, &write_access).await;
        }
        write_access.ignore_events.clone()
    }

    /// Removes expired ignore rules. Returns `true` if anything was removed.
    pub async fn gc_expired(&self) -> bool {
        let mut write_access = self.items.lock().await;
        let now = DateTimeAsMicroseconds::now();
        let before = write_access.ignore_events.len();
        write_access.ignore_events.retain(|item| !item.is_expired(now));
        if write_access.ignore_events.len() == before {
            return false;
        }
        persist(&self.path, &write_access).await;
        true
    }

    pub async fn add_ignore_event(&self, item: &IgnoreItemDto) {
        let mut write_access = self.items.lock().await;
        if let Some(existing) = write_access.ignore_events.iter_mut().find(|x| {
            x.level == item.level && x.application == item.application && x.marker == item.marker
        }) {
            if existing.expires_at == item.expires_at {
                return;
            }
            existing.expires_at = item.expires_at;
        } else {
            write_access.ignore_events.push(item.clone());
        }
        persist(&self.path, &write_access).await;
    }

    pub async fn delete_ignore_event(&self, model: &IgnoreWhereModel) {
        let mut write_access = self.items.lock().await;
        let before = write_access.ignore_events.len();
        write_access.ignore_events.retain(|item| !model.matches(item));
        if write_access.ignore_events.len() != before {
            persist(&self.path, &write_access).await;
        }
    }

    pub async fn get_hours_to_gc(&self) -> u64 {
        self.items.lock().await.hours_to_gc
    }

    pub async fn set_hours_to_gc(&self, hours_to_gc: u64) {
        let mut write_access = self.items.lock().await;

        if write_access.hours_to_gc != hours_to_gc {
            write_access.hours_to_gc = hours_to_gc;
            persist(&self.path, &write_access).await;
        }
    }

    pub async fn get_product_scopes(&self) -> ProductScopesSettings {
        self.items.lock().await.product_scopes()
    }

    /// Returns the scopes as they are then
    pub async fn set_default_product_scope(&self, name: String) -> ProductScopesSettings {
        let mut write_access = self.items.lock().await;

        if write_access.default_product_scope != name {
            write_access.default_product_scope = name;
            persist(&self.path, &write_access).await;
        }

        write_access.product_scopes()
    }

    /// A scope of the same name is replaced where it is - the order is the order they are
    /// matched in - a new one goes last. Returns the scopes as they are then.
    pub async fn set_product_scope(&self, scope: ProductScopeDto) -> ProductScopesSettings {
        let mut write_access = self.items.lock().await;

        match write_access
            .product_scopes
            .iter_mut()
            .find(|existing| existing.name == scope.name)
        {
            Some(existing) => *existing = scope,
            None => write_access.product_scopes.push(scope),
        }

        persist(&self.path, &write_access).await;
        write_access.product_scopes()
    }

    /// Returns the scopes as they are then; `None` - there is no such scope
    pub async fn delete_product_scope(&self, name: &str) -> Option<ProductScopesSettings> {
        let mut write_access = self.items.lock().await;
        let before = write_access.product_scopes.len();
        write_access.product_scopes.retain(|scope| scope.name != name);

        if write_access.product_scopes.len() == before {
            return None;
        }

        persist(&self.path, &write_access).await;
        Some(write_access.product_scopes())
    }
}

async fn persist(path: &str, items: &SettingsFile) {
    let bytes = match serde_json::to_vec_pretty(items) {
        Ok(b) => b,
        Err(e) => {
            println!("Failed to serialize the settings: {}", e);
            return;
        }
    };
    if let Err(e) = tokio::fs::write(path, bytes).await {
        println!("Failed to persist the settings to {}: {}", path, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_file_is_the_array_of_the_ignore_events() {
        let older = br#"[{"level":"Error","application":"billing","marker":"*"}]"#;
        let file = SettingsFile::parse(older);
        assert_eq!(file.ignore_events.len(), 1);
        assert!(file.product_scopes.is_empty());
        assert_eq!(file.default_product_scope, "default");
        assert_eq!(file.hours_to_gc, DEFAULT_HOURS_TO_GC);

        let mut file = file;
        file.default_product_scope = "main".to_string();
        file.hours_to_gc = 48;
        file.product_scopes.push(ProductScopeDto {
            name: "trading".to_string(),
            applications: vec!["trading-.*".to_string()],
        });

        let written = serde_json::to_vec(&file).unwrap();
        let read = SettingsFile::parse(&written);
        assert_eq!(read.ignore_events, file.ignore_events);
        assert_eq!(read.product_scopes(), file.product_scopes());
        assert_eq!(read.hours_to_gc, 48);
    }
}
