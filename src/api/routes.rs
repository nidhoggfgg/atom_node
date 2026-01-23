use super::handlers::{execution, health, plugin};
use super::middleware::cors::add_cors;
use crate::services::{ExecutionService, PluginService};
use axum::{
    Router,
    routing::{delete, get, post, put},
};

#[derive(Clone)]
pub struct AppState {
    pub plugin_service: PluginService,
    pub execution_service: ExecutionService,
}

pub fn create_router(plugin_service: PluginService, execution_service: ExecutionService) -> Router {
    let state = AppState {
        plugin_service,
        execution_service,
    };

    let api_routes = Router::new()
        // Health check
        .route("/health", get(health::health_check))
        // Plugin management
        .route("/api/plugins", get(plugin::list_plugins))
        .route("/api/plugins", post(plugin::install_plugin))
        .route("/api/plugins/{id}", get(plugin::get_plugin))
        .route("/api/plugins/{id}", delete(plugin::uninstall_plugin))
        .route("/api/plugins/{id}/enable", put(plugin::enable_plugin))
        .route("/api/plugins/{id}/disable", put(plugin::disable_plugin))
        // Execution
        .route("/api/plugins/{id}/execute", post(execution::execute_plugin))
        .route("/api/executions", get(execution::list_executions))
        .route("/api/executions/{id}", get(execution::get_execution))
        .route("/api/executions/{id}/stop", put(execution::stop_execution))
        .with_state(state);

    add_cors(api_routes)
}
