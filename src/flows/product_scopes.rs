use crate::{
    app::AppContext,
    product_scopes::{check_scope, to_scope_name, ProductScopes},
    repo::dto::{ProductScopeDto, ProductScopesSettings},
};

/// Sets the scope - a new one goes last, one of the same name is replaced where it is - and
/// keeps it in `settings.json`. The records which come from then on go by the scopes as they
/// are; the written ones stay where they are.
pub async fn set_product_scope(app: &AppContext, scope: ProductScopeDto) -> Result<(), String> {
    check_scope(&scope)?;

    let scope = ProductScopeDto {
        name: scope.name.trim().to_lowercase(),
        applications: scope
            .applications
            .iter()
            .map(|application| application.trim().to_string())
            .filter(|application| !application.is_empty())
            .collect(),
    };

    let settings = app.settings_repo.set_product_scope(scope).await;
    apply(app, &settings);

    Ok(())
}

/// The records of its applications which come from then on go to the default scope, or to
/// the scope of them after it
pub async fn delete_product_scope(app: &AppContext, name: &str) -> Result<(), String> {
    let name = name.trim().to_lowercase();

    match app.settings_repo.delete_product_scope(&name).await {
        Some(settings) => {
            apply(app, &settings);
            Ok(())
        }
        None => Err(format!("There is no product scope '{}'", name)),
    }
}

/// Where the records no scope matches go from then on - and a search which names no scope.
/// The records of the default scope before stay in the scope of its name.
pub async fn set_default_product_scope(app: &AppContext, name: &str) -> Result<String, String> {
    let Some(name) = to_scope_name(name) else {
        return Err(format!(
            "'{}' can not name a product scope: a name is a-z, 0-9, '-' and '_'",
            name
        ));
    };

    let settings = app.settings_repo.set_default_product_scope(name).await;
    apply(app, &settings);

    Ok(settings.default_scope)
}

fn apply(app: &AppContext, settings: &ProductScopesSettings) {
    app.logs_storage.set_product_scopes(ProductScopes::new(
        &settings.default_scope,
        &settings.scopes,
    ));
}
