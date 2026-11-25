-- Tasks definition table
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    prompt TEXT NOT NULL,
    schedule_type TEXT NOT NULL DEFAULT 'once', -- 'once' | 'cron'
    cron_expr TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    next_run_at TEXT,
    max_steps INTEGER NOT NULL DEFAULT 12,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Task execution runs (persistent state machine + checkpointed messages)
CREATE TABLE IF NOT EXISTS task_runs (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued', -- 'queued' | 'running' | 'waiting_approval' | 'completed' | 'failed' | 'cancelled'
    trigger_type TEXT NOT NULL DEFAULT 'manual', -- 'manual' | 'schedule'
    messages_json TEXT NOT NULL DEFAULT '[]',
    pending_tool_call_json TEXT,
    output_summary TEXT,
    error_message TEXT,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    step_count INTEGER NOT NULL DEFAULT 0,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_runs_task_id ON task_runs(task_id);
CREATE INDEX IF NOT EXISTS idx_task_runs_status ON task_runs(status);

-- Execution timeline steps
CREATE TABLE IF NOT EXISTS run_steps (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    step_index INTEGER NOT NULL,
    step_type TEXT NOT NULL, -- 'thought' | 'tool_call' | 'tool_result' | 'approval' | 'answer' | 'error'
    tool_name TEXT,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES task_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_run_steps_run_id ON run_steps(run_id, step_index);

-- Draft-first human-in-the-loop approvals
CREATE TABLE IF NOT EXISTS approvals (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    step_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending' | 'approved' | 'rejected'
    user_feedback TEXT,
    created_at TEXT NOT NULL,
    decided_at TEXT,
    FOREIGN KEY (run_id) REFERENCES task_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_approvals_status ON approvals(status);
CREATE INDEX IF NOT EXISTS idx_approvals_run_id ON approvals(run_id);

-- Personal preferences and long-term memory
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY NOT NULL,
    key TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'preference',
    updated_at TEXT NOT NULL
);

-- Application settings (BYOK LLM config, tool policies)
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
