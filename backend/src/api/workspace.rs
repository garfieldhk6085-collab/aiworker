use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::IntoResponse,
};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    api::AppState,
    error::{AppError, AppResult},
    models::{UploadFileRequest, WorkspaceEntry},
    tools::resolve_safe_path,
};

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    pub path: Option<String>,
}

pub async fn list_workspace_files(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> AppResult<Json<Vec<WorkspaceEntry>>> {
    tokio::fs::create_dir_all(&state.engine.workspace_dir)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rel = query.path.unwrap_or_default();
    let target = resolve_safe_path(&state.engine.workspace_dir, &rel)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    if !target.exists() {
        return Ok(Json(Vec::new()));
    }

    let mut entries = Vec::new();
    let mut rd = tokio::fs::read_dir(&target)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let meta = entry
            .metadata()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let entry_rel = if rel.trim().is_empty() || rel.trim() == "." {
            name.clone()
        } else {
            format!("{}/{}", rel.trim_end_matches('/'), name)
        };
        entries.push(WorkspaceEntry {
            name,
            path: entry_rel,
            is_dir: meta.is_dir(),
            size_bytes: meta.len(),
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(Json(entries))
}

pub async fn upload_workspace_file(
    State(state): State<AppState>,
    Json(req): Json<UploadFileRequest>,
) -> AppResult<Json<Value>> {
    let bytes = if req.is_base64 {
        base64::engine::general_purpose::STANDARD
            .decode(req.content.trim())
            .map_err(|e| AppError::BadRequest(format!("invalid base64 content: {e}")))?
    } else {
        req.content.into_bytes()
    };

    if bytes.len() > 10 * 1024 * 1024 {
        return Err(AppError::BadRequest(
            "file exceeds 10MB upload limit".to_string(),
        ));
    }
    let target = resolve_safe_path(&state.engine.workspace_dir, &req.path)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    tokio::fs::write(&target, &bytes)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(json!({
        "uploaded": true,
        "path": req.path,
        "bytes": bytes.len()
    })))
}

pub async fn download_workspace_file(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> AppResult<impl IntoResponse> {
    let rel = query
        .path
        .ok_or_else(|| AppError::BadRequest("path query parameter is required".to_string()))?;
    let target = resolve_safe_path(&state.engine.workspace_dir, &rel)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if !target.exists() || target.is_dir() {
        return Err(AppError::NotFound(format!("file '{rel}' not found")));
    }
    let bytes = tokio::fs::read(&target)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let filename = target
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(disp) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, disp);
    }
    Ok((headers, bytes))
}

pub async fn delete_workspace_file(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> AppResult<Json<Value>> {
    let rel = query
        .path
        .ok_or_else(|| AppError::BadRequest("path query parameter is required".to_string()))?;
    let target = resolve_safe_path(&state.engine.workspace_dir, &rel)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if !target.exists() {
        return Err(AppError::NotFound(format!("path '{rel}' not found")));
    }
    if target.is_dir() {
        tokio::fs::remove_dir_all(&target)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    } else {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(Json(json!({ "deleted": true, "path": rel })))
}
