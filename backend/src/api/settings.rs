use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::AppState,
    backup::{self, BackupSettings},
    db::{self, now_iso},
    error::{AppError, AppResult},
    models::{LlmSettings, SecretMeta, UpsertSecretRequest},
    notify::{self, CreateNotificationRequest, NotificationChannel, UpdateNotificationRequest},
    sandbox::{self, RuntimeSettings},
};

// ---------------- Secrets Vault ----------------

pub async fn list_secrets(State(state): State<AppState>) -> AppResult<Json<Vec<SecretMeta>>> {
    Ok(Json(db::list_secrets_meta(&state.engine.pool).await?))
}

pub async fn upsert_secret(
    State(state): State<AppState>,
    Json(req): Json<UpsertSecretRequest>,
) -> AppResult<Json<Value>> {
    let name = req.name.trim().to_uppercase();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::BadRequest(
            "secret name must be uppercase letters, digits, or '_' (e.g. GITHUB_TOKEN)".to_string(),
        ));
    }
    if req.value.is_empty() {
        return Err(AppError::BadRequest("secret value cannot be empty".to_string()));
    }
    let id = Uuid::new_v4().to_string();
    let updated_at = now_iso();
    sqlx::query(
        "INSERT INTO secrets (id, name, value, description, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET value = excluded.value, description = excluded.description, updated_at = excluded.updated_at",
    )
    .bind(&id)
    .bind(&name)
    .bind(&req.value)
    .bind(&req.description)
    .bind(&updated_at)
    .execute(&state.engine.pool)
    .await?;

    Ok(Json(json!({ "saved": true, "name": name })))
}

pub async fn delete_secret(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let res = sqlx::query("DELETE FROM secrets WHERE id = ?")
        .bind(&id)
        .execute(&state.engine.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("secret {id} not found")));
    }
    Ok(Json(json!({ "deleted": true, "id": id })))
}

// ---------------- Outbound Notifications ----------------

pub async fn list_notifications(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<NotificationChannel>>> {
    Ok(Json(db::list_notifications(&state.engine.pool).await?))
}

pub async fn create_notification(
    State(state): State<AppState>,
    Json(req): Json<CreateNotificationRequest>,
) -> AppResult<Json<NotificationChannel>> {
    if req.name.trim().is_empty() || req.target_url.trim().is_empty() {
        return Err(AppError::BadRequest(
            "name and target_url are required".to_string(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let events_json = serde_json::to_string(&req.events).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        "INSERT INTO notifications (id, name, channel_type, target_url, events_json, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(req.name.trim())
    .bind(&req.channel_type)
    .bind(req.target_url.trim())
    .bind(&events_json)
    .bind(if req.enabled { 1 } else { 0 })
    .bind(&now)
    .bind(&now)
    .execute(&state.engine.pool)
    .await?;

    Ok(Json(db::get_notification(&state.engine.pool, &id).await?))
}

pub async fn update_notification(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateNotificationRequest>,
) -> AppResult<Json<NotificationChannel>> {
    let mut ch = db::get_notification(&state.engine.pool, &id).await?;
    if let Some(name) = req.name {
        ch.name = name;
    }
    if let Some(ct) = req.channel_type {
        ch.channel_type = ct;
    }
    if let Some(url) = req.target_url {
        ch.target_url = url;
    }
    if let Some(events) = req.events {
        ch.events_json = serde_json::to_string(&events).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(enabled) = req.enabled {
        ch.enabled = if enabled { 1 } else { 0 };
    }
    ch.updated_at = now_iso();

    sqlx::query(
        "UPDATE notifications SET name = ?, channel_type = ?, target_url = ?, events_json = ?, enabled = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&ch.name)
    .bind(&ch.channel_type)
    .bind(&ch.target_url)
    .bind(&ch.events_json)
    .bind(ch.enabled)
    .bind(&ch.updated_at)
    .bind(&ch.id)
    .execute(&state.engine.pool)
    .await?;

    Ok(Json(ch))
}

pub async fn delete_notification(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let res = sqlx::query("DELETE FROM notifications WHERE id = ?")
        .bind(&id)
        .execute(&state.engine.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("notification {id} not found")));
    }
    Ok(Json(json!({ "deleted": true, "id": id })))
}

pub async fn test_notification(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let ch = db::get_notification(&state.engine.pool, &id).await?;
    notify::send_to_channel(
        &ch,
        "test",
        "Notification Test",
        "This is a test notification from aiworker.",
        &json!({ "channel_id": id }),
    )
    .await
    .map_err(AppError::Internal)?;
    Ok(Json(json!({ "sent": true })))
}

// ---------------- Backup & S3 Sync ----------------

pub async fn get_backup_settings(State(state): State<AppState>) -> AppResult<Json<BackupSettings>> {
    let mut cfg = db::get_backup_settings(&state.engine.pool).await?;
    cfg.secret_access_key = db::mask_secret(&cfg.secret_access_key);
    Ok(Json(cfg))
}

pub async fn put_backup_settings(
    State(state): State<AppState>,
    Json(mut incoming): Json<BackupSettings>,
) -> AppResult<Json<BackupSettings>> {
    let existing = db::get_backup_settings(&state.engine.pool).await?;
    if incoming.secret_access_key.trim().starts_with("****") {
        incoming.secret_access_key = existing.secret_access_key;
    }
    if incoming.last_backup_at.is_none() {
        incoming.last_backup_at = existing.last_backup_at;
    }
    if incoming.last_backup_key.is_none() {
        incoming.last_backup_key = existing.last_backup_key;
    }
    db::save_backup_settings(&state.engine.pool, &incoming).await?;
    incoming.secret_access_key = db::mask_secret(&incoming.secret_access_key);
    Ok(Json(incoming))
}

pub async fn export_local_backup(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let (filename, bytes) =
        backup::create_backup_archive(&state.engine.pool, &state.engine.workspace_dir)
            .await
            .map_err(AppError::Internal)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/gzip"),
    );
    if let Ok(disp) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, disp);
    }
    Ok((headers, bytes))
}

pub async fn push_s3_backup(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let mut cfg = db::get_backup_settings(&state.engine.pool).await?;
    let (filename, bytes) =
        backup::create_backup_archive(&state.engine.pool, &state.engine.workspace_dir)
            .await
            .map_err(AppError::Internal)?;
    let size_bytes = bytes.len();
    let key = backup::push_backup_to_s3(&cfg, &filename, bytes)
        .await
        .map_err(AppError::Internal)?;

    let now = now_iso();
    cfg.last_backup_at = Some(now.clone());
    cfg.last_backup_key = Some(key.clone());
    db::save_backup_settings(&state.engine.pool, &cfg).await?;

    Ok(Json(json!({
        "uploaded": true,
        "key": key,
        "size_bytes": size_bytes,
        "timestamp": now
    })))
}

pub async fn list_s3_backups(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<backup::S3BackupObject>>> {
    let cfg = db::get_backup_settings(&state.engine.pool).await?;
    let objects = backup::list_s3_backups(&cfg)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(objects))
}

#[derive(Debug, Deserialize)]
pub struct RestoreS3Request {
    pub key: String,
}

pub async fn restore_s3_backup(
    State(state): State<AppState>,
    Json(req): Json<RestoreS3Request>,
) -> AppResult<Json<backup::RestoreResult>> {
    let cfg = db::get_backup_settings(&state.engine.pool).await?;
    let bytes = backup::download_s3_backup(&cfg, req.key.trim())
        .await
        .map_err(AppError::Internal)?;
    let res = backup::restore_backup_archive(
        bytes,
        &state.engine.workspace_dir,
        &state.engine.data_dir,
    )
    .await
    .map_err(AppError::Internal)?;
    Ok(Json(res))
}

// ---------------- LLM & Runtime Settings ----------------

pub async fn get_llm_settings(State(state): State<AppState>) -> AppResult<Json<LlmSettings>> {
    let mut settings = db::get_llm_settings(&state.engine.pool).await?;
    settings.api_key = db::mask_secret(&settings.api_key);
    Ok(Json(settings))
}

pub async fn put_llm_settings(
    State(state): State<AppState>,
    Json(mut settings): Json<LlmSettings>,
) -> AppResult<Json<LlmSettings>> {
    let existing = db::get_llm_settings(&state.engine.pool).await?;
    if settings.api_key.trim().starts_with("****") {
        settings.api_key = existing.api_key;
    }
    db::save_llm_settings(&state.engine.pool, &settings).await?;
    settings.api_key = db::mask_secret(&settings.api_key);
    Ok(Json(settings))
}

pub async fn get_runtime_settings(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let settings = db::get_runtime_settings(&state.engine.pool).await?;
    let docker_available = sandbox::is_docker_available().await;
    let today_tokens = db::today_tokens_used(&state.engine.pool).await.unwrap_or(0);
    Ok(Json(json!({
        "settings": settings,
        "docker_available": docker_available,
        "today_tokens_used": today_tokens
    })))
}

pub async fn put_runtime_settings(
    State(state): State<AppState>,
    Json(settings): Json<RuntimeSettings>,
) -> AppResult<Json<RuntimeSettings>> {
    db::save_runtime_settings(&state.engine.pool, &settings).await?;
    state
        .engine
        .update_concurrency_limit(settings.max_concurrent_runs)
        .await;
    Ok(Json(settings))
}
