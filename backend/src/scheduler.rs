use std::str::FromStr;

use chrono::{DateTime, Utc};
use cron::Schedule;

use crate::{backup, db, engine::AgentEngine, models::Task};

/// Parse a 5-field (standard Unix) or 6/7-field cron expression and return the next UTC run time.
pub fn compute_next_run(cron_expr: &str, from: DateTime<Utc>) -> Result<String, String> {
    let trimmed = cron_expr.trim();
    let normalized = if trimmed.split_whitespace().count() == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    };

    let schedule = Schedule::from_str(&normalized)
        .map_err(|e| format!("invalid cron expression '{cron_expr}': {e}"))?;

    schedule
        .after(&from)
        .next()
        .map(|dt| dt.to_rfc3339())
        .ok_or_else(|| format!("cron expression '{cron_expr}' has no upcoming execution time"))
}

pub fn spawn_scheduler(engine: AgentEngine) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        let mut tick_counter: u64 = 0;
        let mut next_auto_backup: Option<DateTime<Utc>> = None;

        loop {
            interval.tick().await;
            tick_counter += 1;

            if let Err(err) = tick_due_tasks(&engine).await {
                tracing::error!(error = %err, "scheduler tick failed");
            }

            // Check automatic S3 backup every ~1 minute (4 ticks * 15s)
            if tick_counter % 4 == 0 {
                tick_auto_backup(&engine, &mut next_auto_backup).await;
            }

            // Run retention cleanup once every ~6 hours (1440 ticks * 15s)
            if tick_counter % 1440 == 1 {
                if let Ok(runtime) = db::get_runtime_settings(&engine.pool).await {
                    if let Ok(purged) =
                        db::purge_expired_runs(&engine.pool, runtime.retention_days).await
                    {
                        if purged > 0 {
                            tracing::info!(
                                purged,
                                retention_days = runtime.retention_days,
                                "purged expired task runs"
                            );
                        }
                    }
                }
            }
        }
    });
}

async fn tick_auto_backup(engine: &AgentEngine, next_slot: &mut Option<DateTime<Utc>>) {
    let Ok(mut cfg) = db::get_backup_settings(&engine.pool).await else {
        return;
    };
    if !cfg.enabled || cfg.bucket.trim().is_empty() || cfg.auto_backup_cron.trim().is_empty() {
        *next_slot = None;
        return;
    }

    let now = Utc::now();
    if next_slot.is_none() {
        if let Ok(next_iso) = compute_next_run(&cfg.auto_backup_cron, now) {
            if let Ok(dt) = DateTime::parse_from_rfc3339(&next_iso) {
                *next_slot = Some(dt.with_timezone(&Utc));
            }
        }
        return;
    }

    if let Some(due) = *next_slot {
        if now >= due {
            // Advance schedule first to avoid double-triggering
            *next_slot = compute_next_run(&cfg.auto_backup_cron, now)
                .ok()
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc));

            match backup::create_backup_archive(&engine.pool, &engine.workspace_dir).await {
                Ok((filename, bytes)) => {
                    match backup::push_backup_to_s3(&cfg, &filename, bytes).await {
                        Ok(key) => {
                            cfg.last_backup_at = Some(db::now_iso());
                            cfg.last_backup_key = Some(key.clone());
                            let _ = db::save_backup_settings(&engine.pool, &cfg).await;
                            tracing::info!(s3_key = %key, "automatic S3 backup succeeded");
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "automatic S3 backup upload failed");
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(error = %err, "automatic backup archive creation failed");
                }
            }
        }
    }
}

async fn tick_due_tasks(engine: &AgentEngine) -> anyhow::Result<()> {
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    let due_tasks = sqlx::query_as::<_, Task>(
        "SELECT * FROM tasks WHERE enabled = 1 AND schedule_type = 'cron' AND next_run_at IS NOT NULL AND next_run_at <= ?",
    )
    .bind(&now_str)
    .fetch_all(&engine.pool)
    .await?;

    for task in due_tasks {
        let next_run = task
            .cron_expr
            .as_deref()
            .and_then(|expr| compute_next_run(expr, now).ok());

        sqlx::query("UPDATE tasks SET next_run_at = ?, updated_at = ? WHERE id = ?")
            .bind(&next_run)
            .bind(&now_str)
            .bind(&task.id)
            .execute(&engine.pool)
            .await?;

        tracing::info!(task_id = %task.id, title = %task.title, "triggering scheduled task");
        if let Err(err) = engine.trigger_task(&task, "schedule").await {
            tracing::error!(task_id = %task.id, error = %err, "failed to trigger scheduled task");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_five_field_cron() {
        let next = compute_next_run("0 9 * * *", Utc::now());
        assert!(next.is_ok());
    }
}
