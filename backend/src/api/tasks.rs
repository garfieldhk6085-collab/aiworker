use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::AppState,
    db::{self, now_iso},
    error::{AppError, AppResult},
    models::{CreateTaskRequest, Task, TaskRun, UpdateTaskRequest},
    scheduler::compute_next_run,
};

pub async fn list_tasks(State(state): State<AppState>) -> AppResult<Json<Vec<Task>>> {
    Ok(Json(db::list_tasks(&state.engine.pool).await?))
}

pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Task>> {
    Ok(Json(db::get_task(&state.engine.pool, &id).await?))
}

pub async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> AppResult<Json<Value>> {
    let prompt = req.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(AppError::BadRequest("prompt cannot be empty".to_string()));
    }

    let title = req
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            let short: String = prompt.chars().take(36).collect();
            if prompt.chars().count() > 36 {
                format!("{short}...")
            } else {
                short
            }
        });

    let schedule_type = if req.schedule_type == "cron" {
        "cron".to_string()
    } else {
        "once".to_string()
    };

    let next_run_at = if schedule_type == "cron" {
        let expr = req.cron_expr.as_deref().ok_or_else(|| {
            AppError::BadRequest("cron_expr is required when schedule_type is 'cron'".to_string())
        })?;
        Some(compute_next_run(expr, Utc::now()).map_err(AppError::BadRequest)?)
    } else {
        None
    };

    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let max_steps = req.max_steps.clamp(2, 30);
    let webhook_token = if req.enable_webhook {
        Some(format!("wh_{}", Uuid::new_v4().simple()))
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO tasks (id, title, prompt, schedule_type, cron_expr, enabled, next_run_at, max_steps, worker_id, webhook_token, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&title)
    .bind(&prompt)
    .bind(&schedule_type)
    .bind(&req.cron_expr)
    .bind(&next_run_at)
    .bind(max_steps)
    .bind(&req.worker_id)
    .bind(&webhook_token)
    .bind(&now)
    .bind(&now)
    .execute(&state.engine.pool)
    .await?;

    let task = db::get_task(&state.engine.pool, &id).await?;
    let initial_run = if req.run_immediately {
        Some(state.engine.trigger_task(&task, "manual").await?)
    } else {
        None
    };

    Ok(Json(json!({
        "task": task,
        "run": initial_run
    })))
}

pub async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> AppResult<Json<Task>> {
    let mut task = db::get_task(&state.engine.pool, &id).await?;

    if let Some(title) = req.title {
        task.title = title;
    }
    if let Some(prompt) = req.prompt {
        task.prompt = prompt;
    }
    if let Some(st) = req.schedule_type {
        task.schedule_type = if st == "cron" { "cron".into() } else { "once".into() };
    }
    if let Some(expr) = req.cron_expr {
        task.cron_expr = if expr.trim().is_empty() { None } else { Some(expr) };
    }
    if let Some(enabled) = req.enabled {
        task.enabled = if enabled { 1 } else { 0 };
    }
    if let Some(max_steps) = req.max_steps {
        task.max_steps = max_steps.clamp(2, 30);
    }
    if let Some(worker_id) = req.worker_id {
        task.worker_id = if worker_id.trim().is_empty() {
            None
        } else {
            Some(worker_id)
        };
    }
    if req.regenerate_webhook == Some(true) {
        task.webhook_token = Some(format!("wh_{}", Uuid::new_v4().simple()));
    }

    task.next_run_at = if task.schedule_type == "cron" && task.enabled == 1 {
        let expr = task
            .cron_expr
            .as_deref()
            .ok_or_else(|| AppError::BadRequest("cron_expr is required for cron tasks".to_string()))?;
        Some(compute_next_run(expr, Utc::now()).map_err(AppError::BadRequest)?)
    } else {
        None
    };

    task.updated_at = now_iso();

    sqlx::query(
        "UPDATE tasks SET title = ?, prompt = ?, schedule_type = ?, cron_expr = ?, enabled = ?, next_run_at = ?, max_steps = ?, worker_id = ?, webhook_token = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&task.title)
    .bind(&task.prompt)
    .bind(&task.schedule_type)
    .bind(&task.cron_expr)
    .bind(task.enabled)
    .bind(&task.next_run_at)
    .bind(task.max_steps)
    .bind(&task.worker_id)
    .bind(&task.webhook_token)
    .bind(&task.updated_at)
    .bind(&task.id)
    .execute(&state.engine.pool)
    .await?;

    Ok(Json(task))
}

pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let res = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(&id)
        .execute(&state.engine.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("task {id} not found")));
    }
    Ok(Json(json!({ "deleted": true, "id": id })))
}

pub async fn trigger_task_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<TaskRun>> {
    let task = db::get_task(&state.engine.pool, &id).await?;
    let run = state.engine.trigger_task(&task, "manual").await?;
    Ok(Json(run))
}

pub async fn trigger_inbound_webhook(
    State(state): State<AppState>,
    Path(token): Path<String>,
    payload: Option<Json<Value>>,
) -> AppResult<Json<TaskRun>> {
    let task = db::get_task_by_webhook_token(&state.engine.pool, &token).await?;
    let extra = payload.map(|Json(v)| v);
    let run = state
        .engine
        .trigger_task_with_input(&task, "webhook", extra)
        .await?;
    Ok(Json(run))
}
