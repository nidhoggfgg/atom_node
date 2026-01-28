use crate::api::dto::update::{UpdateRequest, UpdateResponse};
use crate::api::routes::AppState;
use crate::error::Result;
use axum::{Json, extract::State, http::StatusCode};

pub async fn stage_update(
    State(state): State<AppState>,
    Json(req): Json<UpdateRequest>,
) -> Result<(StatusCode, Json<UpdateResponse>)> {
    let status = state.update_service.stage_update(req.package_url).await?;

    let response = UpdateResponse {
        status: "staged".to_string(),
        restart_required: status.restart_required,
        current_version: status.current_version,
        package_version: status.package_version,
    };

    Ok((StatusCode::ACCEPTED, Json(response)))
}
