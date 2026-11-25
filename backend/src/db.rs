use std::{collections::HashMap, str::FromStr};

use chrono::{Duration, Utc};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use uuid::Uuid;

use crate::{
    backup::BackupSettings,
    error::{AppError, AppResult},
    models::{
        Approval, ApprovalWithContext, LlmSettings, MemoryItem, RunStep, SecretMeta, SecretRow,
        Task, TaskRun, TaskRunWithTask, ToolPolicy, Worker,
    },
    notify::NotificationChannel,
    sandbox::RuntimeSettings,
};

pub const LLM_SETTINGS_KEY: &str = "llm_settings";
pub const TOOL_POLICIES_KEY: &str = "tool_policies";
pub const RUNTIME_SETTINGS_KEY: &str = "runtime_settings";
pub const BACKUP_SETTINGS_KEY: &str = "backup_settings";

pub async fn init_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub async fn list_tasks(pool: &SqlitePool) -> AppResult<Vec<Task>> {
    let tasks = sqlx::query_as::<_, Task>("SELECT * FROM tasks ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(tasks)
}

pub async fn get_task(pool: &SqlitePool, task_id: &str) -> AppResult<Task> {
    sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {task_id} not found")))
}

pub async fn get_task_by_webhook_token(pool: &SqlitePool, token: &str) -> AppResult<Task> {
    sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE webhook_token = ?")
        .bind(token)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("invalid webhook token".to_string()))
}

pub async fn list_workers(pool: &SqlitePool) -> AppResult<Vec<Worker>> {
    let items = sqlx::query_as::<_, Worker>("SELECT * FROM workers ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(items)
}

pub async fn get_worker(pool: &SqlitePool, worker_id: &str) -> AppResult<Worker> {
    sqlx::query_as::<_, Worker>("SELECT * FROM workers WHERE id = ?")
        .bind(worker_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("worker {worker_id} not found")))
}

pub async fn list_secrets_meta(pool: &SqlitePool) -> AppResult<Vec<SecretMeta>> {
    let rows = sqlx::query_as::<_, SecretRow>("SELECT * FROM secrets ORDER BY name ASC")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let masked = if r.value.len() <= 4 {
                "****".to_string()
            } else {
                let tail: String = r
                    .value
                    .chars()
                    .rev()
                    .take(4)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                format!("****{tail}")
            };
            SecretMeta {
                id: r.id,
                name: r.name,
                description: r.description,
                masked_value: masked,
                updated_at: r.updated_at,
            }
        })
        .collect())
}

pub async fn load_secrets_map(pool: &SqlitePool) -> AppResult<HashMap<String, String>> {
    let rows = sqlx::query_as::<_, SecretRow>("SELECT * FROM secrets")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.name, r.value)).collect())
}

pub async fn list_notifications(pool: &SqlitePool) -> AppResult<Vec<NotificationChannel>> {
    let items = sqlx::query_as::<_, NotificationChannel>(
        "SELECT * FROM notifications ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(items)
}

pub async fn get_notification(pool: &SqlitePool, id: &str) -> AppResult<NotificationChannel> {
    sqlx::query_as::<_, NotificationChannel>("SELECT * FROM notifications WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("notification {id} not found")))
}

pub async fn list_runs(pool: &SqlitePool, limit: i64) -> AppResult<Vec<TaskRunWithTask>> {
    let rows = sqlx::query_as::<_, TaskRun>(
        "SELECT * FROM task_runs ORDER BY started_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::with_capacity(rows.len());
    for run in rows {
        let task = sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = ?")
            .bind(&run.task_id)
            .fetch_optional(pool)
            .await?;
        let (task_title, task_prompt) = match task {
            Some(t) => (t.title, t.prompt),
            None => ("Deleted Task".to_string(), String::new()),
        };
        result.push(TaskRunWithTask {
            run,
            task_title,
            task_prompt,
        });
    }
    Ok(result)
}

pub async fn get_run(pool: &SqlitePool, run_id: &str) -> AppResult<TaskRun> {
    sqlx::query_as::<_, TaskRun>("SELECT * FROM task_runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("run {run_id} not found")))
}

pub async fn today_tokens_used(pool: &SqlitePool) -> AppResult<i64> {
    let day_prefix = Utc::now().format("%Y-%m-%d").to_string();
    let like_pattern = format!("{day_prefix}%");
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_tokens), 0) FROM task_runs WHERE started_at LIKE ?",
    )
    .bind(like_pattern)
    .fetch_one(pool)
    .await?;
    Ok(total.unwrap_or(0))
}

pub async fn purge_expired_runs(pool: &SqlitePool, retention_days: i64) -> AppResult<u64> {
    if retention_days <= 0 {
        return Ok(0);
    }
    let cutoff = (Utc::now() - Duration::days(retention_days)).to_rfc3339();
    let res = sqlx::query(
        "DELETE FROM task_runs WHERE status IN ('completed', 'failed', 'cancelled') AND finished_at IS NOT NULL AND finished_at < ?",
    )
    .bind(cutoff)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn list_run_steps(pool: &SqlitePool, run_id: &str) -> AppResult<Vec<RunStep>> {
    let steps = sqlx::query_as::<_, RunStep>(
        "SELECT * FROM run_steps WHERE run_id = ? ORDER BY step_index ASC, created_at ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    Ok(steps)
}

pub async fn insert_run_step(
    pool: &SqlitePool,
    run_id: &str,
    step_index: i64,
    step_type: &str,
    tool_name: Option<&str>,
    payload: &serde_json::Value,
) -> AppResult<RunStep> {
    let id = Uuid::new_v4().to_string();
    let created_at = now_iso();
    let payload_json = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());

    sqlx::query(
        "INSERT INTO run_steps (id, run_id, step_index, step_type, tool_name, payload_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(run_id)
    .bind(step_index)
    .bind(step_type)
    .bind(tool_name)
    .bind(&payload_json)
    .bind(&created_at)
    .execute(pool)
    .await?;

    Ok(RunStep {
        id,
        run_id: run_id.to_string(),
        step_index,
        step_type: step_type.to_string(),
        tool_name: tool_name.map(|s| s.to_string()),
        payload_json,
        created_at,
    })
}

pub async fn list_approvals(
    pool: &SqlitePool,
    status_filter: Option<&str>,
) -> AppResult<Vec<ApprovalWithContext>> {
    let approvals = match status_filter {
        Some(status) => {
            sqlx::query_as::<_, Approval>(
                "SELECT * FROM approvals WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(status)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, Approval>(
                "SELECT * FROM approvals ORDER BY created_at DESC LIMIT 100",
            )
            .fetch_all(pool)
            .await?
        }
    };

    let mut out = Vec::with_capacity(approvals.len());
    for approval in approvals {
        let run = sqlx::query_as::<_, TaskRun>("SELECT * FROM task_runs WHERE id = ?")
            .bind(&approval.run_id)
            .fetch_optional(pool)
            .await?;
        let (task_id, task_title) = if let Some(run) = run {
            let title = sqlx::query_scalar::<_, String>("SELECT title FROM tasks WHERE id = ?")
                .bind(&run.task_id)
                .fetch_optional(pool)
                .await?
                .unwrap_or_else(|| "Task".to_string());
            (run.task_id, title)
        } else {
            (String::new(), "Task".to_string())
        };
        out.push(ApprovalWithContext {
            approval,
            task_id,
            task_title,
        });
    }
    Ok(out)
}

pub async fn list_run_approvals(pool: &SqlitePool, run_id: &str) -> AppResult<Vec<Approval>> {
    let items = sqlx::query_as::<_, Approval>(
        "SELECT * FROM approvals WHERE run_id = ? ORDER BY created_at ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    Ok(items)
}

pub async fn get_llm_settings(pool: &SqlitePool) -> AppResult<LlmSettings> {
    let raw = sqlx::query_scalar::<_, String>("SELECT value_json FROM settings WHERE key = ?")
        .bind(LLM_SETTINGS_KEY)
        .fetch_optional(pool)
        .await?;

    match raw {
        Some(json_str) => Ok(serde_json::from_str(&json_str).unwrap_or_default()),
        None => Ok(LlmSettings::default()),
    }
}

pub async fn save_llm_settings(pool: &SqlitePool, settings: &LlmSettings) -> AppResult<()> {
    save_setting_json(pool, LLM_SETTINGS_KEY, settings).await
}

pub async fn get_runtime_settings(pool: &SqlitePool) -> AppResult<RuntimeSettings> {
    let raw = sqlx::query_scalar::<_, String>("SELECT value_json FROM settings WHERE key = ?")
        .bind(RUNTIME_SETTINGS_KEY)
        .fetch_optional(pool)
        .await?;

    match raw {
        Some(json_str) => Ok(serde_json::from_str(&json_str).unwrap_or_default()),
        None => Ok(RuntimeSettings::default()),
    }
}

pub async fn save_runtime_settings(pool: &SqlitePool, settings: &RuntimeSettings) -> AppResult<()> {
    save_setting_json(pool, RUNTIME_SETTINGS_KEY, settings).await
}

pub async fn get_backup_settings(pool: &SqlitePool) -> AppResult<BackupSettings> {
    let raw = sqlx::query_scalar::<_, String>("SELECT value_json FROM settings WHERE key = ?")
        .bind(BACKUP_SETTINGS_KEY)
        .fetch_optional(pool)
        .await?;

    match raw {
        Some(json_str) => Ok(serde_json::from_str(&json_str).unwrap_or_default()),
        None => Ok(BackupSettings::default()),
    }
}

pub async fn save_backup_settings(pool: &SqlitePool, settings: &BackupSettings) -> AppResult<()> {
    save_setting_json(pool, BACKUP_SETTINGS_KEY, settings).await
}

pub fn mask_secret(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= 4 {
        return "****".to_string();
    }
    let tail: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("****{tail}")
}

async fn save_setting_json<T: serde::Serialize>(
    pool: &SqlitePool,
    key: &str,
    val: &T,
) -> AppResult<()> {
    let value_json = serde_json::to_string(val)
        .map_err(|e| AppError::BadRequest(format!("invalid settings: {e}")))?;
    let updated_at = now_iso();
    sqlx::query(
        "INSERT INTO settings (key, value_json, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value_json)
    .bind(updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_tool_policies(pool: &SqlitePool) -> AppResult<HashMap<String, ToolPolicy>> {
    let raw = sqlx::query_scalar::<_, String>("SELECT value_json FROM settings WHERE key = ?")
        .bind(TOOL_POLICIES_KEY)
        .fetch_optional(pool)
        .await?;

    match raw {
        Some(json_str) => Ok(serde_json::from_str(&json_str).unwrap_or_default()),
        None => Ok(HashMap::new()),
    }
}

pub async fn save_tool_policy(
    pool: &SqlitePool,
    tool_name: &str,
    policy: ToolPolicy,
) -> AppResult<()> {
    let mut map = get_tool_policies(pool).await?;
    map.insert(tool_name.to_string(), policy);
    save_setting_json(pool, TOOL_POLICIES_KEY, &map).await
}

pub async fn list_memories(pool: &SqlitePool) -> AppResult<Vec<MemoryItem>> {
    let items = sqlx::query_as::<_, MemoryItem>("SELECT * FROM memories ORDER BY updated_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(items)
}
