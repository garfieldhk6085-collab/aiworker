use std::{path::PathBuf, sync::Arc};

use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::sync::{RwLock, Semaphore, broadcast};
use uuid::Uuid;

use crate::{
    db::{self, now_iso},
    error::{AppError, AppResult},
    llm,
    models::{ChatMessage, LlmSettings, RunEvent, Task, TaskRun, ToolCall, ToolPolicy, Worker},
    notify,
    skills,
    tools::{ToolContext, ToolRegistry},
};

#[derive(Clone)]
pub struct AgentEngine {
    pub pool: SqlitePool,
    pub tools: Arc<ToolRegistry>,
    pub data_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub events_tx: broadcast::Sender<RunEvent>,
    concurrency_limit: Arc<RwLock<Arc<Semaphore>>>,
}

impl AgentEngine {
    pub fn new(pool: SqlitePool, workspace_dir: PathBuf) -> Self {
        let data_dir = workspace_dir
            .parent()
            .map(|p| p.join("data"))
            .unwrap_or_else(|| PathBuf::from("./data"));
        Self::new_with_dirs(pool, data_dir, workspace_dir, 4)
    }

    pub fn new_with_dirs(
        pool: SqlitePool,
        data_dir: PathBuf,
        workspace_dir: PathBuf,
        max_concurrent_runs: usize,
    ) -> Self {
        let (events_tx, _) = broadcast::channel(256);
        let skills_dir = workspace_dir.join("skills");
        Self {
            pool,
            tools: Arc::new(ToolRegistry::with_builtin_tools()),
            data_dir,
            workspace_dir,
            skills_dir,
            events_tx,
            concurrency_limit: Arc::new(RwLock::new(Arc::new(Semaphore::new(
                max_concurrent_runs.clamp(1, 32),
            )))),
        }
    }

    pub async fn update_concurrency_limit(&self, max_concurrent_runs: usize) {
        let mut guard = self.concurrency_limit.write().await;
        *guard = Arc::new(Semaphore::new(max_concurrent_runs.clamp(1, 32)));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RunEvent> {
        self.events_tx.subscribe()
    }

    /// Crash recovery on startup: mark orphaned queued/running runs as failed while preserving waiting_approval checkpoints.
    pub async fn recover_interrupted_runs(&self) -> anyhow::Result<u64> {
        let finished_at = now_iso();
        let res = sqlx::query(
            "UPDATE task_runs
             SET status = 'failed',
                 error_message = 'Interrupted by server restart',
                 finished_at = ?
             WHERE status IN ('queued', 'running')",
        )
        .bind(&finished_at)
        .execute(&self.pool)
        .await?;
        let count = res.rows_affected();
        if count > 0 {
            tracing::warn!(recovered = count, "marked interrupted runs as failed on startup");
        }
        Ok(count)
    }

    fn emit(&self, run_id: &str, event_type: &str, status: Option<&str>, payload: Value) {
        let _ = self.events_tx.send(RunEvent {
            run_id: run_id.to_string(),
            event_type: event_type.to_string(),
            status: status.map(|s| s.to_string()),
            payload,
            timestamp: now_iso(),
        });
    }

    pub async fn trigger_task(&self, task: &Task, trigger_type: &str) -> AppResult<TaskRun> {
        self.trigger_task_with_input(task, trigger_type, None).await
    }

    /// Create a new run record for a task (with optional inbound webhook payload) and spawn background execution.
    pub async fn trigger_task_with_input(
        &self,
        task: &Task,
        trigger_type: &str,
        extra_input: Option<Value>,
    ) -> AppResult<TaskRun> {
        let run_id = Uuid::new_v4().to_string();
        let started_at = now_iso();

        let worker = if let Some(ref wid) = task.worker_id {
            db::get_worker(&self.pool, wid).await.ok()
        } else {
            None
        };

        let memories = db::list_memories(&self.pool).await.unwrap_or_default();
        let installed_skills = skills::list_skills(&self.skills_dir)
            .await
            .unwrap_or_default();

        let mut system_prompt = worker
            .as_ref()
            .map(|w| w.system_prompt.clone())
            .unwrap_or_else(|| {
                "You are aiworker, a reliable personal AI automation assistant. \
                 Break down the user's goal, use available tools carefully, and deliver concise, structured results in the user's language."
                    .to_string()
            });

        if !memories.is_empty() {
            system_prompt.push_str("\n\nPersonal Context & Preferences:\n");
            for m in memories {
                system_prompt.push_str(&format!("- [{}] {}: {}\n", m.category, m.key, m.content));
            }
        }

        if !installed_skills.is_empty() {
            system_prompt.push_str(
                "\n\nAvailable Skills (call `skill_read` with the skill name to load detailed instructions when relevant):\n",
            );
            for s in installed_skills {
                system_prompt.push_str(&format!("- {}: {}\n", s.name, s.description));
            }
        }

        let user_prompt = match extra_input {
            Some(payload) if !payload.is_null() => {
                let pretty = serde_json::to_string_pretty(&payload).unwrap_or_default();
                format!(
                    "{}\n\n[Inbound Trigger Payload]\n```json\n{}\n```",
                    task.prompt, pretty
                )
            }
            _ => task.prompt.clone(),
        };

        let initial_messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(system_prompt),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(user_prompt),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];

        let messages_json = serde_json::to_string(&initial_messages)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize messages: {e}")))?;

        sqlx::query(
            "INSERT INTO task_runs (id, task_id, status, trigger_type, messages_json, total_tokens, step_count, started_at)
             VALUES (?, ?, 'queued', ?, ?, 0, 0, ?)",
        )
        .bind(&run_id)
        .bind(&task.id)
        .bind(trigger_type)
        .bind(&messages_json)
        .bind(&started_at)
        .execute(&self.pool)
        .await?;

        let run = db::get_run(&self.pool, &run_id).await?;
        self.emit(
            &run_id,
            "status",
            Some("queued"),
            json!({ "task_id": task.id, "trigger_type": trigger_type }),
        );

        let engine = self.clone();
        let run_id_bg = run_id.clone();
        tokio::spawn(async move {
            if let Err(err) = engine.execute_loop(&run_id_bg).await {
                tracing::error!(run_id = %run_id_bg, error = %err, "agent run loop failed");
            }
        });

        Ok(run)
    }

    /// Process a human decision on a pending approval and resume the paused run
    /// (preserving any remaining tool calls from the same assistant turn).
    pub async fn decide_approval(
        &self,
        approval_id: &str,
        decision: &str,
        feedback: Option<String>,
    ) -> AppResult<TaskRun> {
        if decision != "approved" && decision != "rejected" {
            return Err(AppError::BadRequest(
                "decision must be 'approved' or 'rejected'".to_string(),
            ));
        }

        let approval = sqlx::query_as::<_, crate::models::Approval>(
            "SELECT * FROM approvals WHERE id = ?",
        )
        .bind(approval_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("approval {approval_id} not found")))?;

        if approval.status != "pending" {
            return Err(AppError::BadRequest(format!(
                "approval {approval_id} is already {}",
                approval.status
            )));
        }

        let decided_at = now_iso();
        sqlx::query(
            "UPDATE approvals SET status = ?, user_feedback = ?, decided_at = ? WHERE id = ?",
        )
        .bind(decision)
        .bind(&feedback)
        .bind(&decided_at)
        .bind(approval_id)
        .execute(&self.pool)
        .await?;

        let run = db::get_run(&self.pool, &approval.run_id).await?;
        if run.status != "waiting_approval" {
            return Ok(run);
        }

        let mut messages: Vec<ChatMessage> =
            serde_json::from_str(&run.messages_json).unwrap_or_default();
        let mut pending_queue = parse_pending_calls(run.pending_tool_call_json.as_deref());

        let mut next_step = run.step_count;

        if !pending_queue.is_empty() {
            let call = pending_queue.remove(0);
            let secrets = db::load_secrets_map(&self.pool).await.unwrap_or_default();
            let runtime = db::get_runtime_settings(&self.pool).await.unwrap_or_default();
            let ctx = ToolContext {
                workspace_dir: &self.workspace_dir,
                skills_dir: &self.skills_dir,
                secrets: &secrets,
                runtime: &runtime,
            };

            let tool_output = if decision == "approved" {
                let args: Value =
                    serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));
                match self.tools.get(&call.function.name) {
                    Some(tool) => match tool.execute(args, &ctx).await {
                        Ok(val) => json!({ "status": "approved", "result": val }),
                        Err(err) => json!({ "status": "error", "error": err.to_string() }),
                    },
                    None => json!({ "status": "error", "error": "tool not found" }),
                }
            } else {
                json!({
                    "status": "rejected",
                    "feedback": feedback.clone().unwrap_or_else(|| "User rejected this action.".to_string())
                })
            };

            next_step += 1;
            let step = db::insert_run_step(
                &self.pool,
                &run.id,
                next_step,
                "tool_result",
                Some(&call.function.name),
                &json!({
                    "approval_decision": decision,
                    "feedback": feedback,
                    "output": tool_output
                }),
            )
            .await?;

            self.emit(
                &run.id,
                "step",
                Some("running"),
                serde_json::to_value(&step).unwrap_or_default(),
            );

            messages.push(ChatMessage {
                role: "tool".to_string(),
                content: Some(serde_json::to_string(&tool_output).unwrap_or_default()),
                tool_calls: None,
                tool_call_id: Some(call.id),
                name: Some(call.function.name),
            });
        }

        let messages_json = serde_json::to_string(&messages).unwrap_or_else(|_| "[]".to_string());
        let remaining_json = if pending_queue.is_empty() {
            None
        } else {
            serde_json::to_string(&pending_queue).ok()
        };

        sqlx::query(
            "UPDATE task_runs SET status = 'running', messages_json = ?, pending_tool_call_json = ?, step_count = ? WHERE id = ?",
        )
        .bind(&messages_json)
        .bind(&remaining_json)
        .bind(next_step)
        .bind(&run.id)
        .execute(&self.pool)
        .await?;

        self.emit(
            &run.id,
            "status",
            Some("running"),
            json!({ "resumed_from_approval": approval_id, "decision": decision }),
        );

        let engine = self.clone();
        let run_id_bg = run.id.clone();
        tokio::spawn(async move {
            if let Err(err) = engine.execute_loop(&run_id_bg).await {
                tracing::error!(run_id = %run_id_bg, error = %err, "resumed run loop failed");
            }
        });

        db::get_run(&self.pool, &run.id).await
    }

    pub async fn cancel_run(&self, run_id: &str) -> AppResult<TaskRun> {
        let run = db::get_run(&self.pool, run_id).await?;
        if run.status == "completed" || run.status == "failed" || run.status == "cancelled" {
            return Ok(run);
        }
        let finished_at = now_iso();
        sqlx::query(
            "UPDATE task_runs SET status = 'cancelled', pending_tool_call_json = NULL, finished_at = ? WHERE id = ?",
        )
        .bind(&finished_at)
        .bind(run_id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "UPDATE approvals SET status = 'rejected', user_feedback = 'Run cancelled', decided_at = ? WHERE run_id = ? AND status = 'pending'",
        )
        .bind(&finished_at)
        .bind(run_id)
        .execute(&self.pool)
        .await?;

        self.emit(run_id, "status", Some("cancelled"), json!({}));
        db::get_run(&self.pool, run_id).await
    }

    /// Core durable agent loop: drains any remaining tool calls from a resumed turn,
    /// then runs LLM turns until final answer, approval pause, cancellation, budget limit, or max_steps.
    async fn execute_loop(&self, run_id: &str) -> anyhow::Result<()> {
        let sem = self.concurrency_limit.read().await.clone();
        let _permit = sem
            .acquire_owned()
            .await
            .map_err(|e| anyhow::anyhow!("concurrency semaphore closed: {e}"))?;

        let run = db::get_run(&self.pool, run_id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let task = db::get_task(&self.pool, &run.task_id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if run.status == "cancelled" || run.status == "completed" || run.status == "failed" {
            return Ok(());
        }

        sqlx::query("UPDATE task_runs SET status = 'running' WHERE id = ?")
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        self.emit(run_id, "status", Some("running"), json!({}));

        let worker: Option<Worker> = if let Some(ref wid) = task.worker_id {
            db::get_worker(&self.pool, wid).await.ok()
        } else {
            None
        };

        let mut settings: LlmSettings = db::get_llm_settings(&self.pool).await.unwrap_or_default();
        if let Some(ref w) = worker {
            if let Some(ref p) = w.provider {
                if !p.trim().is_empty() {
                    settings.provider = p.clone();
                }
            }
            if let Some(ref m) = w.model {
                if !m.trim().is_empty() {
                    settings.model = m.clone();
                }
            }
            if let Some(ref u) = w.base_url {
                if !u.trim().is_empty() {
                    settings.base_url = u.clone();
                }
            }
        }

        let allowed_tools: Option<Vec<String>> = worker
            .as_ref()
            .and_then(|w| w.allowed_tools_json.as_deref())
            .and_then(|s| serde_json::from_str(s).ok());

        let runtime = db::get_runtime_settings(&self.pool).await.unwrap_or_default();
        let policies = db::get_tool_policies(&self.pool).await.unwrap_or_default();
        let tool_defs = self
            .tools
            .openai_tool_definitions(&policies, allowed_tools.as_deref());

        let mut messages: Vec<ChatMessage> =
            serde_json::from_str(&run.messages_json).unwrap_or_default();
        let mut step_count = run.step_count;
        let mut total_tokens = run.total_tokens;
        let max_steps = worker
            .as_ref()
            .and_then(|w| w.max_steps)
            .unwrap_or(task.max_steps)
            .max(2);

        // First, finish any remaining tool calls from a previously paused multi-tool turn
        let remaining_from_pause = parse_pending_calls(run.pending_tool_call_json.as_deref());
        if !remaining_from_pause.is_empty() {
            sqlx::query("UPDATE task_runs SET pending_tool_call_json = NULL WHERE id = ?")
                .bind(run_id)
                .execute(&self.pool)
                .await?;

            let paused = self
                .process_tool_calls(
                    &task,
                    run_id,
                    remaining_from_pause,
                    &allowed_tools,
                    &policies,
                    &runtime,
                    &mut messages,
                    &mut step_count,
                    total_tokens,
                )
                .await?;
            if paused {
                return Ok(());
            }
            let messages_json =
                serde_json::to_string(&messages).unwrap_or_else(|_| "[]".to_string());
            sqlx::query(
                "UPDATE task_runs SET messages_json = ?, total_tokens = ?, step_count = ? WHERE id = ?",
            )
            .bind(&messages_json)
            .bind(total_tokens)
            .bind(step_count)
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        }

        loop {
            let current_status: String =
                sqlx::query_scalar("SELECT status FROM task_runs WHERE id = ?")
                    .bind(run_id)
                    .fetch_one(&self.pool)
                    .await?;
            if current_status == "cancelled" {
                return Ok(());
            }

            if runtime.daily_token_budget > 0 {
                let used_today = db::today_tokens_used(&self.pool).await.unwrap_or(0);
                if used_today >= runtime.daily_token_budget {
                    let err_msg = format!(
                        "Daily token budget exceeded ({used_today} / {}). Adjust limit in Runtime Settings.",
                        runtime.daily_token_budget
                    );
                    self.fail_run(&task, run_id, step_count, total_tokens, &messages, &err_msg)
                        .await?;
                    return Ok(());
                }
            }

            if step_count >= max_steps {
                let err_msg = format!("Reached max step limit ({max_steps}).");
                self.fail_run(&task, run_id, step_count, total_tokens, &messages, &err_msg)
                    .await?;
                return Ok(());
            }

            let resp = match llm::complete_chat(&settings, &messages, &tool_defs).await {
                Ok(r) => r,
                Err(err) => {
                    self.fail_run(
                        &task,
                        run_id,
                        step_count,
                        total_tokens,
                        &messages,
                        &err.to_string(),
                    )
                    .await?;
                    return Ok(());
                }
            };

            total_tokens += resp.total_tokens;

            if let Some(ref thought) = resp.content {
                if !thought.trim().is_empty() && !resp.tool_calls.is_empty() {
                    step_count += 1;
                    let step = db::insert_run_step(
                        &self.pool,
                        run_id,
                        step_count,
                        "thought",
                        None,
                        &json!({ "text": thought }),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                    self.emit(
                        run_id,
                        "step",
                        Some("running"),
                        serde_json::to_value(&step).unwrap_or_default(),
                    );
                }
            }

            messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: resp.content.clone(),
                tool_calls: if resp.tool_calls.is_empty() {
                    None
                } else {
                    Some(resp.tool_calls.clone())
                },
                tool_call_id: None,
                name: None,
            });

            if resp.tool_calls.is_empty() {
                let summary = resp
                    .content
                    .unwrap_or_else(|| "Task completed with no output.".to_string());
                step_count += 1;
                let step = db::insert_run_step(
                    &self.pool,
                    run_id,
                    step_count,
                    "answer",
                    None,
                    &json!({ "text": summary }),
                )
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

                let finished_at = now_iso();
                let messages_json =
                    serde_json::to_string(&messages).unwrap_or_else(|_| "[]".to_string());
                sqlx::query(
                    "UPDATE task_runs SET status = 'completed', messages_json = ?, output_summary = ?, total_tokens = ?, step_count = ?, finished_at = ? WHERE id = ?",
                )
                .bind(&messages_json)
                .bind(&summary)
                .bind(total_tokens)
                .bind(step_count)
                .bind(&finished_at)
                .bind(run_id)
                .execute(&self.pool)
                .await?;

                self.emit(
                    run_id,
                    "completed",
                    Some("completed"),
                    json!({ "step": step, "output_summary": summary, "total_tokens": total_tokens }),
                );
                notify::dispatch_event(
                    self.pool.clone(),
                    "completed".to_string(),
                    task.title.clone(),
                    summary,
                    json!({ "task_id": task.id, "run_id": run_id, "total_tokens": total_tokens }),
                );
                return Ok(());
            }

            let paused = self
                .process_tool_calls(
                    &task,
                    run_id,
                    resp.tool_calls,
                    &allowed_tools,
                    &policies,
                    &runtime,
                    &mut messages,
                    &mut step_count,
                    total_tokens,
                )
                .await?;

            if paused {
                return Ok(());
            }

            let messages_json =
                serde_json::to_string(&messages).unwrap_or_else(|_| "[]".to_string());
            sqlx::query(
                "UPDATE task_runs SET messages_json = ?, total_tokens = ?, step_count = ? WHERE id = ?",
            )
            .bind(&messages_json)
            .bind(total_tokens)
            .bind(step_count)
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        }
    }

    /// Execute a list of tool calls sequentially. Returns `Ok(true)` if execution paused for approval.
    #[allow(clippy::too_many_arguments)]
    async fn process_tool_calls(
        &self,
        task: &Task,
        run_id: &str,
        calls: Vec<ToolCall>,
        allowed_tools: &Option<Vec<String>>,
        policies: &std::collections::HashMap<String, ToolPolicy>,
        runtime: &crate::sandbox::RuntimeSettings,
        messages: &mut Vec<ChatMessage>,
        step_count: &mut i64,
        total_tokens: i64,
    ) -> anyhow::Result<bool> {
        let secrets = db::load_secrets_map(&self.pool).await.unwrap_or_default();
        let tool_ctx = ToolContext {
            workspace_dir: &self.workspace_dir,
            skills_dir: &self.skills_dir,
            secrets: &secrets,
            runtime,
        };

        for idx in 0..calls.len() {
            let call = &calls[idx];
            let tool_name = call.function.name.clone();
            let args: Value =
                serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));

            *step_count += 1;
            let call_step = db::insert_run_step(
                &self.pool,
                run_id,
                *step_count,
                "tool_call",
                Some(&tool_name),
                &json!({ "call_id": call.id, "arguments": args }),
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            self.emit(
                run_id,
                "step",
                Some("running"),
                serde_json::to_value(&call_step).unwrap_or_default(),
            );

            if let Some(allowed) = allowed_tools {
                if !allowed.iter().any(|n| n == &tool_name) {
                    let out = json!({ "error": format!("tool '{tool_name}' is not permitted for this worker") });
                    messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(out.to_string()),
                        tool_calls: None,
                        tool_call_id: Some(call.id.clone()),
                        name: Some(tool_name),
                    });
                    continue;
                }
            }

            let Some(tool) = self.tools.get(&tool_name) else {
                let err_payload = json!({ "error": format!("unknown tool: {tool_name}") });
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(err_payload.to_string()),
                    tool_calls: None,
                    tool_call_id: Some(call.id.clone()),
                    name: Some(tool_name),
                });
                continue;
            };

            let effective_policy = policies
                .get(&tool_name)
                .copied()
                .unwrap_or_else(|| tool.default_policy());

            match effective_policy {
                ToolPolicy::Disabled => {
                    let out = json!({ "error": format!("tool '{tool_name}' is disabled by user policy") });
                    *step_count += 1;
                    let step = db::insert_run_step(
                        &self.pool,
                        run_id,
                        *step_count,
                        "tool_result",
                        Some(&tool_name),
                        &json!({ "output": out }),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                    self.emit(
                        run_id,
                        "step",
                        Some("running"),
                        serde_json::to_value(&step).unwrap_or_default(),
                    );
                    messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(out.to_string()),
                        tool_calls: None,
                        tool_call_id: Some(call.id.clone()),
                        name: Some(tool_name),
                    });
                }
                ToolPolicy::RequireApproval => {
                    let approval_id = Uuid::new_v4().to_string();
                    let created_at = now_iso();
                    let reason = format!(
                        "Tool '{}' ({}) requires your confirmation before executing.",
                        tool_name,
                        tool.risk_level()
                    );
                    let args_str = serde_json::to_string_pretty(&args)
                        .unwrap_or_else(|_| "{}".to_string());

                    sqlx::query(
                        "INSERT INTO approvals (id, run_id, step_id, tool_name, arguments_json, reason, status, created_at)
                         VALUES (?, ?, ?, ?, ?, ?, 'pending', ?)",
                    )
                    .bind(&approval_id)
                    .bind(run_id)
                    .bind(&call_step.id)
                    .bind(&tool_name)
                    .bind(&args_str)
                    .bind(&reason)
                    .bind(&created_at)
                    .execute(&self.pool)
                    .await?;

                    let messages_json =
                        serde_json::to_string(messages).unwrap_or_else(|_| "[]".to_string());
                    // Persist current call + all remaining calls in this turn so tool_call_id pairing never breaks
                    let remaining_calls: Vec<ToolCall> = calls[idx..].to_vec();
                    let pending_calls_json =
                        serde_json::to_string(&remaining_calls).unwrap_or_else(|_| "[]".to_string());

                    sqlx::query(
                        "UPDATE task_runs SET status = 'waiting_approval', messages_json = ?, pending_tool_call_json = ?, total_tokens = ?, step_count = ? WHERE id = ?",
                    )
                    .bind(&messages_json)
                    .bind(&pending_calls_json)
                    .bind(total_tokens)
                    .bind(*step_count)
                    .bind(run_id)
                    .execute(&self.pool)
                    .await?;

                    self.emit(
                        run_id,
                        "approval_needed",
                        Some("waiting_approval"),
                        json!({
                            "approval_id": approval_id,
                            "tool_name": tool_name,
                            "arguments": args,
                            "reason": reason
                        }),
                    );
                    notify::dispatch_event(
                        self.pool.clone(),
                        "approval_needed".to_string(),
                        task.title.clone(),
                        format!("等待审批工具 `{tool_name}`：{reason}"),
                        json!({ "task_id": task.id, "run_id": run_id, "approval_id": approval_id }),
                    );
                    return Ok(true);
                }
                ToolPolicy::AutoAllow => {
                    let output = match tool.execute(args, &tool_ctx).await {
                        Ok(val) => json!({ "ok": true, "result": val }),
                        Err(err) => json!({ "ok": false, "error": err.to_string() }),
                    };

                    *step_count += 1;
                    let res_step = db::insert_run_step(
                        &self.pool,
                        run_id,
                        *step_count,
                        "tool_result",
                        Some(&tool_name),
                        &json!({ "output": output }),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                    self.emit(
                        run_id,
                        "step",
                        Some("running"),
                        serde_json::to_value(&res_step).unwrap_or_default(),
                    );

                    messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(output.to_string()),
                        tool_calls: None,
                        tool_call_id: Some(call.id.clone()),
                        name: Some(tool_name),
                    });
                }
            }
        }

        Ok(false)
    }

    async fn fail_run(
        &self,
        task: &Task,
        run_id: &str,
        step_count: i64,
        total_tokens: i64,
        messages: &[ChatMessage],
        error_msg: &str,
    ) -> anyhow::Result<()> {
        let next_step = step_count + 1;
        let _ = db::insert_run_step(
            &self.pool,
            run_id,
            next_step,
            "error",
            None,
            &json!({ "error": error_msg }),
        )
        .await;

        let finished_at = now_iso();
        let messages_json = serde_json::to_string(messages).unwrap_or_else(|_| "[]".to_string());
        sqlx::query(
            "UPDATE task_runs SET status = 'failed', messages_json = ?, error_message = ?, total_tokens = ?, step_count = ?, finished_at = ? WHERE id = ?",
        )
        .bind(&messages_json)
        .bind(error_msg)
        .bind(total_tokens)
        .bind(next_step)
        .bind(&finished_at)
        .bind(run_id)
        .execute(&self.pool)
        .await?;

        self.emit(run_id, "failed", Some("failed"), json!({ "error": error_msg }));
        notify::dispatch_event(
            self.pool.clone(),
            "failed".to_string(),
            task.title.clone(),
            error_msg.to_string(),
            json!({ "task_id": task.id, "run_id": run_id }),
        );
        Ok(())
    }
}

/// Backward-compatible parser for `pending_tool_call_json` (supports Vec<ToolCall> or legacy single ToolCall).
fn parse_pending_calls(raw: Option<&str>) -> Vec<ToolCall> {
    let Some(s) = raw else {
        return Vec::new();
    };
    if let Ok(list) = serde_json::from_str::<Vec<ToolCall>>(s) {
        return list;
    }
    if let Ok(single) = serde_json::from_str::<ToolCall>(s) {
        return vec![single];
    }
    Vec::new()
}
