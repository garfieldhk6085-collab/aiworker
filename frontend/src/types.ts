export interface Task {
  id: string;
  title: string;
  prompt: string;
  schedule_type: 'once' | 'cron';
  cron_expr: string | null;
  enabled: number;
  next_run_at: string | null;
  max_steps: number;
  worker_id: string | null;
  webhook_token: string | null;
  created_at: string;
  updated_at: string;
}

export interface Worker {
  id: string;
  name: string;
  description: string;
  system_prompt: string;
  provider: string | null;
  model: string | null;
  base_url: string | null;
  allowed_tools_json: string | null;
  max_steps: number | null;
  created_at: string;
  updated_at: string;
}

export interface SkillSummary {
  name: string;
  description: string;
  files: string[];
}

export interface SkillDetail extends SkillSummary {
  content: string;
}

export interface SecretMeta {
  id: string;
  name: string;
  description: string;
  masked_value: string;
  updated_at: string;
}

export interface NotificationChannel {
  id: string;
  name: string;
  channel_type: 'webhook' | 'feishu' | 'slack' | 'discord';
  target_url: string;
  events_json: string;
  enabled: number;
  created_at: string;
  updated_at: string;
}

export interface WorkspaceEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size_bytes: number;
}

export interface RuntimeSettings {
  sandbox_mode: 'auto' | 'docker' | 'local';
  docker_image: string;
  docker_network: 'none' | 'bridge';
  default_timeout_secs: number;
  max_concurrent_runs: number;
  daily_token_budget: number;
  retention_days: number;
}

export interface RuntimeStatusResponse {
  settings: RuntimeSettings;
  docker_available: boolean;
  today_tokens_used: number;
}

export type RunStatus =
  | 'queued'
  | 'running'
  | 'waiting_approval'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface TaskRun {
  id: string;
  task_id: string;
  status: RunStatus;
  trigger_type: string;
  output_summary: string | null;
  error_message: string | null;
  total_tokens: number;
  step_count: number;
  started_at: string;
  finished_at: string | null;
}

export interface TaskRunWithTask extends TaskRun {
  task_title: string;
  task_prompt: string;
}

export interface RunStep {
  id: string;
  run_id: string;
  step_index: number;
  step_type: 'thought' | 'tool_call' | 'tool_result' | 'approval' | 'answer' | 'error';
  tool_name: string | null;
  payload_json: string;
  created_at: string;
}

export interface Approval {
  id: string;
  run_id: string;
  step_id: string;
  tool_name: string;
  arguments_json: string;
  reason: string;
  status: 'pending' | 'approved' | 'rejected';
  user_feedback: string | null;
  created_at: string;
  decided_at: string | null;
}

export interface ApprovalWithContext extends Approval {
  task_id: string;
  task_title: string;
}

export interface RunDetail {
  run: TaskRun;
  task: Task;
  steps: RunStep[];
  approvals: Approval[];
}

export type ToolPolicy = 'auto_allow' | 'require_approval' | 'disabled';

export interface ToolInfo {
  name: string;
  description: string;
  risk_level: 'read_only' | 'write' | 'external' | 'exec';
  default_policy: ToolPolicy;
  effective_policy: ToolPolicy;
  parameters_schema: Record<string, unknown>;
}

export interface MemoryItem {
  id: string;
  key: string;
  content: string;
  category: string;
  updated_at: string;
}

export interface LlmSettings {
  provider: 'mock' | 'openai' | 'anthropic' | 'gemini' | 'ollama' | 'openai_compatible';
  base_url: string;
  api_key: string;
  model: string;
  temperature: number;
  default_max_steps: number;
}

export interface BackupSettings {
  enabled: boolean;
  endpoint: string;
  region: string;
  bucket: string;
  prefix: string;
  access_key_id: string;
  secret_access_key: string;
  force_path_style: boolean;
  auto_backup_cron: string;
  last_backup_at: string | null;
  last_backup_key: string | null;
}

export interface S3BackupObject {
  key: string;
  size_bytes: number;
  last_modified: string;
}

export interface RestoreResult {
  restored_files: number;
  db_snapshot_saved_to: string | null;
}
