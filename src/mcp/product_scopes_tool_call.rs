use std::sync::Arc;

use mcp_server_middleware::{ApplyJsonSchema, ToolDefinition};
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::{app::AppContext, repo::dto::ProductScopeDto};

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct ManageProductScopeResponse {
    #[property(description: "Human-readable result message.")]
    pub message: String,
}

// ====================== get_product_scopes ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetProductScopesInputData {}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetProductScopesResponse {
    #[property(description: "The scope the records no scope matches go to, and the one searched when a search names no scope.")]
    pub default_scope: String,
    #[property(description: "Product scopes as JSON array string: the default scope first, then the scopes in the order they are matched, then the other scopes there are records of. Each item: {name, applications}. `applications` are the regular expressions an application name matches whole, ignoring case; empty for a scope which is not set.")]
    pub scopes_json: String,
}

pub struct GetProductScopesHandler {
    app: Arc<AppContext>,
}

impl GetProductScopesHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for GetProductScopesHandler {
    const FUNC_NAME: &'static str = "get_product_scopes";
    const DESCRIPTION: &'static str = "List the product scopes. A record goes to the scope its writer names in the `scope` header; without it, to the first scope with a regular expression matching its application whole, ignoring case; otherwise to the default scope. A search goes over one scope.";
}

#[async_trait::async_trait]
impl McpToolCall<GetProductScopesInputData, GetProductScopesResponse> for GetProductScopesHandler {
    async fn execute_tool_call(
        &self,
        _model: GetProductScopesInputData,
    ) -> Result<GetProductScopesResponse, String> {
        let product_scopes = self.app.logs_storage.product_scopes();
        let with_records = self.app.logs_storage.scopes_with_records();

        let scopes: Vec<_> = product_scopes
            .list(&with_records)
            .into_iter()
            .map(|(name, applications)| {
                serde_json::json!({
                    "name": name,
                    "applications": applications,
                })
            })
            .collect();

        Ok(GetProductScopesResponse {
            default_scope: product_scopes.default_scope().to_string(),
            scopes_json: serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".to_string()),
        })
    }
}

// ====================== set_product_scope ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SetProductScopeInputData {
    #[property(description: "Scope name: a-z, 0-9, '-' and '_', at most 64 characters. Upper case is turned to lower case. A scope of the same name is replaced and keeps its place in the order.")]
    pub name: String,
    #[property(description: "Regular expressions an application name matches whole, ignoring case. Example: [\"trading-.*\", \"quotes\"]. The scopes are matched in the order they were added: the first match wins.")]
    pub applications: Vec<String>,
}

pub struct SetProductScopeHandler {
    app: Arc<AppContext>,
}

impl SetProductScopeHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for SetProductScopeHandler {
    const FUNC_NAME: &'static str = "set_product_scope";
    const DESCRIPTION: &'static str = "Create a product scope, or replace the one of the same name. The records which come from then on go by it; the records written before stay in their scopes.";
}

#[async_trait::async_trait]
impl McpToolCall<SetProductScopeInputData, ManageProductScopeResponse> for SetProductScopeHandler {
    async fn execute_tool_call(
        &self,
        model: SetProductScopeInputData,
    ) -> Result<ManageProductScopeResponse, String> {
        let scope = ProductScopeDto {
            name: model.name,
            applications: model.applications,
        };

        let name = scope.name.trim().to_lowercase();
        crate::flows::set_product_scope(&self.app, scope).await?;

        Ok(ManageProductScopeResponse {
            message: format!("Product scope '{}' is set", name),
        })
    }
}

// ====================== delete_product_scope ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct DeleteProductScopeInputData {
    #[property(description: "Name of the scope to delete.")]
    pub name: String,
}

pub struct DeleteProductScopeHandler {
    app: Arc<AppContext>,
}

impl DeleteProductScopeHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for DeleteProductScopeHandler {
    const FUNC_NAME: &'static str = "delete_product_scope";
    const DESCRIPTION: &'static str = "Delete a product scope. The records of its applications which come from then on go to the scope after it they match, or to the default scope. Its records written before stay searchable by its name.";
}

#[async_trait::async_trait]
impl McpToolCall<DeleteProductScopeInputData, ManageProductScopeResponse>
    for DeleteProductScopeHandler
{
    async fn execute_tool_call(
        &self,
        model: DeleteProductScopeInputData,
    ) -> Result<ManageProductScopeResponse, String> {
        crate::flows::delete_product_scope(&self.app, &model.name).await?;

        Ok(ManageProductScopeResponse {
            message: format!(
                "Product scope '{}' is deleted",
                model.name.trim().to_lowercase()
            ),
        })
    }
}

// ====================== set_default_product_scope ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SetDefaultProductScopeInputData {
    #[property(description: "Scope name: a-z, 0-9, '-' and '_', at most 64 characters. It may be a scope which is set: then its applications and the ones no scope matches go to it.")]
    pub name: String,
}

pub struct SetDefaultProductScopeHandler {
    app: Arc<AppContext>,
}

impl SetDefaultProductScopeHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for SetDefaultProductScopeHandler {
    const FUNC_NAME: &'static str = "set_default_product_scope";
    const DESCRIPTION: &'static str = "Set the default product scope: where the records no scope matches go, and the scope searched when a search names none. It is 'default' until it is set. The records of the default scope before stay searchable by its name.";
}

#[async_trait::async_trait]
impl McpToolCall<SetDefaultProductScopeInputData, ManageProductScopeResponse>
    for SetDefaultProductScopeHandler
{
    async fn execute_tool_call(
        &self,
        model: SetDefaultProductScopeInputData,
    ) -> Result<ManageProductScopeResponse, String> {
        let name = crate::flows::set_default_product_scope(&self.app, &model.name).await?;

        Ok(ManageProductScopeResponse {
            message: format!("The default product scope is '{}'", name),
        })
    }
}
