use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub schedule_type: String, // "once" | "cron"
    pub cron_expr: Option<String>,
    pub enabled: i64,
    pub next_run_at: Option<String>,
    pub max_steps: i64,
    pub worker_id: Option<String>,
    pub webhook_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTaskRequest {
    pub title: Option<String>,
    pub prompt: String,
    #[serde(default = "default_schedule_type")]
    pub schedule_type: String,
    pub cron_expr: Option<String>,
    #[serde(default = "default_max_steps")]
    pub max_steps: i64,
    pub worker_id: Option<String>,
    #[serde(default)]
    pub enable_webhook: bool,
    #[serde(default = "default_true")]
    pub run_immediately: bool,
}

fn default_schedule_type() -> String {
    "once".to_string()
}

fn default_max_steps() -> i64 {
    12
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub schedule_type: Option<String>,
    pub cron_expr: Option<String>,
    pub enabled: Option<bool>,
    pub max_steps: Option<i64>,
    pub worker_id: Option<String>,
    pub regenerate_webhook: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Worker {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub allowed_tools_json: Option<String>,
    pub max_steps: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkerRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub system_prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_steps: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWorkerRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_steps: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SecretRow {
    pub id: String,
    pub name: String,
    pub value: String,
    pub description: String,
    pub updated_at: String,
}

/// Safe public view of a secret (never exposes plaintext value)
#[derive(Debug, Clone, Serialize)]
pub struct SecretMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub masked_value: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertSecretRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadFileRequest {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub is_base64: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskRun {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub trigger_type: String,
    pub messages_json: String,
    pub pending_tool_call_json: Option<String>,
    pub output_summary: Option<String>,
    pub error_message: Option<String>,
    pub total_tokens: i64,
    pub step_count: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRunWithTask {
    #[serde(flatten)]
    pub run: TaskRun,
    pub task_title: String,
    pub task_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RunStep {
    pub id: String,
    pub run_id: String,
    pub step_index: i64,
    pub step_type: String,
    pub tool_name: Option<String>,
    pub payload_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunDetailResponse {
    pub run: TaskRun,
    pub task: Task,
    pub steps: Vec<RunStep>,
    pub approvals: Vec<Approval>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Approval {
    pub id: String,
    pub run_id: String,
    pub step_id: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub reason: String,
    pub status: String, // "pending" | "approved" | "rejected"
    pub user_feedback: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalWithContext {
    #[serde(flatten)]
    pub approval: Approval,
    pub task_id: String,
    pub task_title: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecideApprovalRequest {
    pub decision: String, // "approved" | "rejected"
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MemoryItem {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertMemoryRequest {
    pub key: String,
    pub content: String,
    #[serde(default = "default_memory_category")]
    pub category: String,
}

fn default_memory_category() -> String {
    "preference".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    pub provider: String, // "mock" | "openai" | "anthropic" | "gemini" | "ollama" | "openai_compatible"
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub default_max_steps: i64,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            provider: "mock".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
            temperature: 0.2,
            default_max_steps: 12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicy {
    AutoAllow,
    RequireApproval,
    Disabled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub risk_level: String, // "read_only" | "write" | "external" | "exec"
    pub default_policy: ToolPolicy,
    pub effective_policy: ToolPolicy,
    pub parameters_schema: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateToolPolicyRequest {
    pub policy: ToolPolicy,
}

/// Chat message stored inside task_runs.messages_json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system" | "user" | "assistant" | "tool"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Real-time SSE event broadcasted to frontend clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub run_id: String,
    pub event_type: String, // "status" | "step" | "approval_needed" | "completed" | "failed"
    pub status: Option<String>,
    pub payload: Value,
    pub timestamp: String,
}
