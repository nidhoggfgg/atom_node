use crate::api::dto::execution::{ExecutePluginRequest, ExecutionResponse, ExecutionsListResponse};
use crate::api::routes::AppState;
use crate::error::Result;
use axum::{
    Json,
    extract::{Path, Query, State},
};

pub async fn execute_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    Json(req): Json<ExecutePluginRequest>,
) -> Result<Json<ExecutionResponse>> {
    let params = req.params.unwrap_or_default();

    let execution = state
        .execution_service
        .execute_plugin(&plugin_id, params)
        .await?;
    Ok(Json(ExecutionResponse::from(execution)))
}

pub async fn get_execution(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ExecutionResponse>> {
    let execution = state.execution_service.get_execution(&id).await?;
    Ok(Json(ExecutionResponse::from(execution)))
}

pub async fn list_executions(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ExecutionsListResponse>> {
    let plugin_id = params.get("plugin_id").cloned();

    let executions = state.execution_service.list_executions(plugin_id).await?;
    let response = ExecutionsListResponse {
        data: executions
            .into_iter()
            .map(ExecutionResponse::from)
            .collect(),
    };
    Ok(Json(response))
}

pub async fn stop_execution(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state.execution_service.stop_execution(&id).await?;
    Ok(Json(serde_json::json!({
        "message": "Execution stopped"
    })))
}
