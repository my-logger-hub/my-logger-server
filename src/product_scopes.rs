use std::{
    collections::{BTreeSet, HashMap},
    sync::RwLock,
};

use regex::Regex;

use crate::{app::LogItem, repo::dto::ProductScopeDto};

/// The default scope when `settings.json` names none
pub const DEFAULT_PRODUCT_SCOPE: &str = "default";

/// The scope of this many applications is remembered; the ones after are matched each time
const MAX_REMEMBERED_APPLICATIONS: usize = 10_000;

pub struct ProductScope {
    pub name: String,
    /// The regular expressions as they are set
    pub applications: Vec<String>,
    regexes: Vec<Regex>,
}

/// The product scopes of `settings.json`: the records of an application go to the first one
/// with a regular expression matching its name whole, ignoring case - to the default scope
/// when none does or the record names no application.
pub struct ProductScopes {
    default_scope: String,
    scopes: Vec<ProductScope>,
    /// Application → the index of its scope, `None` for the default one
    remembered: RwLock<HashMap<String, Option<usize>>>,
}

impl ProductScopes {
    /// A default scope which can not name a folder is `default`. A scope with such a name
    /// is left out, and so is a regular expression which does not compile.
    pub fn new(default_scope: &str, settings: &[ProductScopeDto]) -> Self {
        let default_scope = match to_scope_name(default_scope) {
            Some(name) => name,
            None => {
                println!(
                    "Default product scope '{}' is '{}': a name is a-z, 0-9, '-' and '_'",
                    default_scope, DEFAULT_PRODUCT_SCOPE
                );
                DEFAULT_PRODUCT_SCOPE.to_string()
            }
        };

        let mut scopes = Vec::new();

        for scope in settings {
            let Some(name) = to_scope_name(&scope.name) else {
                println!(
                    "Product scope '{}' is left out: a name is a-z, 0-9, '-' and '_'",
                    scope.name
                );
                continue;
            };

            let mut applications = Vec::new();
            let mut regexes = Vec::new();

            for pattern in &scope.applications {
                match Regex::new(&format!("(?i)^(?:{})$", pattern)) {
                    Ok(regex) => {
                        applications.push(pattern.clone());
                        regexes.push(regex);
                    }
                    Err(err) => println!(
                        "Application '{}' of product scope '{}' is left out: {}",
                        pattern, name, err
                    ),
                }
            }

            scopes.push(ProductScope {
                name,
                applications,
                regexes,
            });
        }

        Self {
            default_scope,
            scopes,
            remembered: RwLock::new(HashMap::new()),
        }
    }

    /// Where the records no scope matches go - and a search which names no scope
    pub fn default_scope(&self) -> &str {
        &self.default_scope
    }

    pub fn scope_of(&self, application: Option<&str>) -> &str {
        let Some(application) = application else {
            return &self.default_scope;
        };

        if let Some(index) = self.remembered.read().unwrap().get(application) {
            return self.name_of(*index);
        }

        let index = self.scopes.iter().position(|scope| {
            scope
                .regexes
                .iter()
                .any(|regex| regex.is_match(application))
        });

        let mut remembered = self.remembered.write().unwrap();

        if remembered.len() < MAX_REMEMBERED_APPLICATIONS {
            remembered.insert(application.to_string(), index);
        }

        self.name_of(index)
    }

    /// The scope a record goes to: the one its writer names, or the one of its application
    pub fn record_scope<'s>(&'s self, named: Option<&'s str>, item: &LogItem) -> &'s str {
        match named {
            Some(scope) => scope,
            None => self.scope_of(item.ctx.get("Application").map(String::as_str)),
        }
    }

    fn name_of(&self, index: Option<usize>) -> &str {
        index
            .and_then(|index| self.scopes.get(index))
            .map_or(self.default_scope.as_str(), |scope| scope.name.as_str())
    }

    /// The scope a search names: the default one when it names none
    pub fn requested(&self, scope: Option<&str>) -> Result<String, String> {
        match scope.map(str::trim) {
            None | Some("") => Ok(self.default_scope.clone()),
            Some(scope) => to_scope_name(scope).ok_or_else(|| {
                format!(
                    "'{}' is not a product scope: a name is a-z, 0-9, '-' and '_'",
                    scope
                )
            }),
        }
    }

    /// The default scope first - with the regular expressions of the scope of its name, if
    /// one is set - then the scopes as they are set, then the other ones there are records
    /// of: each name with its regular expressions
    pub fn list<'s>(&'s self, with_records: &'s BTreeSet<String>) -> Vec<(&'s str, &'s [String])> {
        let default_applications = self
            .scopes
            .iter()
            .find(|scope| scope.name == self.default_scope)
            .map_or([].as_slice(), |scope| scope.applications.as_slice());

        let mut result = vec![(self.default_scope.as_str(), default_applications)];

        for scope in &self.scopes {
            if scope.name != self.default_scope {
                result.push((scope.name.as_str(), scope.applications.as_slice()));
            }
        }

        for name in with_records {
            if !result.iter().any(|(listed, _)| listed == name) {
                result.push((name.as_str(), [].as_slice()));
            }
        }

        result
    }
}

/// A scope names a folder: a-z, 0-9, '-' and '_'
pub fn is_valid_scope_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// The name as a scope is named - trimmed, in lower case - if it can name one
pub fn to_scope_name(name: &str) -> Option<String> {
    let name = name.trim().to_lowercase();
    is_valid_scope_name(&name).then_some(name)
}

/// Why the scope can not be set as it is: a name which can not name a folder, a regular
/// expression which does not compile
pub fn check_scope(scope: &ProductScopeDto) -> Result<(), String> {
    if to_scope_name(&scope.name).is_none() {
        return Err(format!(
            "'{}' can not name a product scope: a name is a-z, 0-9, '-' and '_'",
            scope.name
        ));
    }

    for pattern in &scope.applications {
        if let Err(err) = Regex::new(&format!("(?i)^(?:{})$", pattern)) {
            return Err(format!("'{}' is not a regular expression: {}", pattern, err));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(name: &str, applications: &[&str]) -> ProductScopeDto {
        ProductScopeDto {
            name: name.to_string(),
            applications: applications.iter().map(|app| app.to_string()).collect(),
        }
    }

    #[test]
    fn the_first_scope_matching_the_whole_name_takes_the_records() {
        let scopes = ProductScopes::new(
            DEFAULT_PRODUCT_SCOPE,
            &[
                scope("Trading", &["trading-.*", "quotes"]),
                scope("everything", &[".*"]),
                scope("bad name!", &[".*"]),
                scope("broken", &["(", "billing"]),
            ],
        );

        assert_eq!(scopes.scope_of(Some("Trading-Api")), "trading");
        assert_eq!(scopes.scope_of(Some("QUOTES")), "trading");
        // Not the whole name
        assert_eq!(scopes.scope_of(Some("quotes-feed")), "everything");
        assert_eq!(scopes.scope_of(None), DEFAULT_PRODUCT_SCOPE);

        let with_records = BTreeSet::from(["trading".to_string(), "by-header".to_string()]);
        let list = scopes.list(&with_records);
        let names: Vec<&str> = list.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["default", "trading", "everything", "broken", "by-header"]);
        assert_eq!(list[3].1, ["billing".to_string()]);
    }

    #[test]
    fn nothing_matched_is_the_default_scope() {
        let scopes = ProductScopes::new(" Main ", &[scope("trading", &["trading-.*"])]);
        assert_eq!(scopes.default_scope(), "main");
        assert_eq!(scopes.scope_of(Some("billing")), "main");
        // Remembered: the same answer the second time
        assert_eq!(scopes.scope_of(Some("billing")), "main");

        let scopes = ProductScopes::new("../etc", &[]);
        assert_eq!(scopes.default_scope(), DEFAULT_PRODUCT_SCOPE);
    }

    #[test]
    fn the_default_scope_may_have_applications_of_its_own() {
        let scopes = ProductScopes::new(
            "main",
            &[scope("trading", &["trading-.*"]), scope("main", &["billing"])],
        );

        assert_eq!(scopes.scope_of(Some("billing")), "main");
        assert_eq!(scopes.scope_of(Some("quotes")), "main");

        let with_records = BTreeSet::new();
        let list = scopes.list(&with_records);
        assert_eq!(list.len(), 2);
        let billing = ["billing".to_string()];
        assert_eq!(list[0], ("main", billing.as_slice()));
    }

    #[test]
    fn a_requested_scope_names_a_folder() {
        let scopes = ProductScopes::new("main", &[]);
        assert_eq!(scopes.requested(None).unwrap(), "main");
        assert_eq!(scopes.requested(Some(" ")).unwrap(), "main");
        assert_eq!(scopes.requested(Some(" Trading ")).unwrap(), "trading");
        assert!(scopes.requested(Some("../etc")).is_err());
    }
}
