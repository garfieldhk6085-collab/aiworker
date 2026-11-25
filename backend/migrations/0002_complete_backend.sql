-- Worker personas & model routing presets
CREATE TABLE IF NOT EXISTS workers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    system_prompt TEXT NOT NULL,
    provider TEXT,          -- optional override: mock | openai | anthropic | gemini | ollama | openai_compatible
    model TEXT,             -- optional model override
    base_url TEXT,          -- optional base_url override
    allowed_tools_json TEXT, -- optional JSON array of allowed tool names; NULL means all enabled tools
    max_steps INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Extend tasks with optional worker binding and inbound webhook token
ALTER TABLE tasks ADD COLUMN worker_id TEXT;
ALTER TABLE tasks ADD COLUMN webhook_token TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_webhook_token ON tasks(webhook_token) WHERE webhook_token IS NOT NULL;

-- Secrets vault (injected into sandbox env only, never echoed back in plain text)
CREATE TABLE IF NOT EXISTS secrets (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
);

-- Outbound notification channels (Webhook / Feishu / Slack / Discord)
CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL DEFAULT 'webhook', -- 'webhook' | 'feishu' | 'slack' | 'discord'
    target_url TEXT NOT NULL,
    events_json TEXT NOT NULL DEFAULT '["approval_needed","completed","failed"]',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
