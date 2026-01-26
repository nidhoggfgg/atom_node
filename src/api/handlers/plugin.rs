use crate::api::dto::plugin::{
    InstallPluginRequest, PluginResponse, PluginsListResponse, UpdatePluginRequest,
};
use crate::api::routes::AppState;
use crate::error::Result;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

pub async fn list_plugins(State(state): State<AppState>) -> Result<Json<PluginsListResponse>> {
    let plugins = state.plugin_service.list_plugins().await?;
    let data = plugins
        .into_iter()
        .map(PluginResponse::try_from)
        .collect::<Result<Vec<_>>>()?;
    let response = PluginsListResponse { data };
    Ok(Json(response))
}

pub async fn get_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PluginResponse>> {
    let plugin = state.plugin_service.get_plugin(&id).await?;
    Ok(Json(PluginResponse::try_from(plugin)?))
}

pub async fn install_plugin(
    State(state): State<AppState>,
    Json(req): Json<InstallPluginRequest>,
) -> Result<(StatusCode, Json<PluginResponse>)> {
    let plugin = state
        .plugin_service
        .install_plugin(req.package_url)
        .await?;

    Ok((StatusCode::CREATED, Json(PluginResponse::try_from(plugin)?)))
}

pub async fn uninstall_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    state.plugin_service.uninstall_plugin(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePluginRequest>,
) -> Result<(StatusCode, Json<PluginResponse>)> {
    let plugin = state
        .plugin_service
        .update_plugin(&id, req.package_url)
        .await?;
    Ok((StatusCode::OK, Json(PluginResponse::try_from(plugin)?)))
}

pub async fn enable_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    state.plugin_service.enable_plugin(&id).await?;
    Ok(StatusCode::OK)
}

pub async fn disable_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    state.plugin_service.disable_plugin(&id).await?;
    Ok(StatusCode::OK)
}
