use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use reqwest::Method;
use serde_json::{Value, json};

use crate::{
    models::{ToolInfo, ToolPolicy},
    sandbox::{self, RuntimeSettings},
    skills,
};

pub struct ToolContext<'a> {
    pub workspace_dir: &'a Path,
    pub skills_dir: &'a Path,
    pub secrets: &'a HashMap<String, String>,
    pub runtime: &'a RuntimeSettings,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn risk_level(&self) -> &'static str; // "read_only" | "write" | "external" | "exec"
    fn default_policy(&self) -> ToolPolicy;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, args: Value, ctx: &ToolContext<'_>) -> anyhow::Result<Value>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    order: Vec<String>,
}

impl ToolRegistry {
    pub fn with_builtin_tools() -> Self {
        let mut reg = Self {
            tools: HashMap::new(),
            order: Vec::new(),
        };
        reg.register(Arc::new(WorkspaceReadTool));
        reg.register(Arc::new(WorkspaceWriteTool));
        reg.register(Arc::new(SkillReadTool));
        reg.register(Arc::new(WebFetchTool));
        reg.register(Arc::new(HttpRequestTool));
        reg.register(Arc::new(PythonExecTool));
        reg.register(Arc::new(ShellExecTool));
        reg
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        if !self.tools.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.tools.insert(name, tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_info(&self, overrides: &HashMap<String, ToolPolicy>) -> Vec<ToolInfo> {
        self.order
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|t| {
                let effective_policy = overrides
                    .get(t.name())
                    .copied()
                    .unwrap_or_else(|| t.default_policy());
                ToolInfo {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    risk_level: t.risk_level().to_string(),
                    default_policy: t.default_policy(),
                    effective_policy,
                    parameters_schema: t.parameters_schema(),
                }
            })
            .collect()
    }

    pub fn openai_tool_definitions(
        &self,
        overrides: &HashMap<String, ToolPolicy>,
        allowed_filter: Option<&[String]>,
    ) -> Vec<Value> {
        self.list_info(overrides)
            .into_iter()
            .filter(|info| info.effective_policy != ToolPolicy::Disabled)
            .filter(|info| match allowed_filter {
                Some(allowed) => allowed.iter().any(|n| n == &info.name),
                None => true,
            })
            .map(|info| {
                json!({
                    "type": "function",
                    "function": {
                        "name": info.name,
                        "description": info.description,
                        "parameters": info.parameters_schema
                    }
                })
            })
            .collect()
    }
}

/// Prevent path traversal outside the configured workspace sandbox directory.
pub fn resolve_safe_path(workspace_dir: &Path, relative_path: &str) -> anyhow::Result<PathBuf> {
    let trimmed = relative_path.trim().trim_start_matches('/');
    let rel = Path::new(trimmed);
    for comp in rel.components() {
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!("invalid path: path traversal is not allowed"));
            }
            _ => {}
        }
    }
    Ok(workspace_dir.join(rel))
}

struct WorkspaceReadTool;

#[async_trait]
impl Tool for WorkspaceReadTool {
    fn name(&self) -> &'static str {
        "workspace_read"
    }

    fn description(&self) -> &'static str {
        "Read a file or list files inside the personal local workspace sandbox directory."
    }

    fn risk_level(&self) -> &'static str {
        "read_only"
    }

    fn default_policy(&self) -> ToolPolicy {
        ToolPolicy::AutoAllow
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path inside workspace. Leave empty or '.' to list files."
                }
            }
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext<'_>) -> anyhow::Result<Value> {
        tokio::fs::create_dir_all(ctx.workspace_dir).await?;
        let raw_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let target = resolve_safe_path(ctx.workspace_dir, raw_path)?;

        if !target.exists() {
            return Ok(json!({
                "exists": false,
                "path": raw_path,
                "message": "File or directory does not exist yet."
            }));
        }

        if target.is_dir() {
            let mut entries = Vec::new();
            let mut rd = tokio::fs::read_dir(&target).await?;
            while let Some(entry) = rd.next_entry().await? {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().await?.is_dir();
                entries.push(json!({ "name": name, "is_dir": is_dir }));
            }
            Ok(json!({
                "path": raw_path,
                "is_dir": true,
                "entries": entries
            }))
        } else {
            let content = tokio::fs::read_to_string(&target).await?;
            let redacted = sandbox::redact_secrets(&content, ctx.secrets);
            let truncated = if redacted.chars().count() > 16_000 {
                let head: String = redacted.chars().take(16_000).collect();
                format!("{head}...\n[truncated]")
            } else {
                redacted
            };
            Ok(json!({
                "path": raw_path,
                "is_dir": false,
                "content": truncated
            }))
        }
    }
}

struct WorkspaceWriteTool;

#[async_trait]
impl Tool for WorkspaceWriteTool {
    fn name(&self) -> &'static str {
        "workspace_write"
    }

    fn description(&self) -> &'static str {
        "Create or overwrite a Markdown note, report, or text file inside the local workspace directory."
    }

    fn risk_level(&self) -> &'static str {
        "write"
    }

    fn default_policy(&self) -> ToolPolicy {
        ToolPolicy::RequireApproval
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative file path inside workspace, e.g. notes/daily-brief.md"
                },
                "content": {
                    "type": "string",
                    "description": "Full text or Markdown content to write"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext<'_>) -> anyhow::Result<Value> {
        let raw_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: path"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: content"))?;

        let target = resolve_safe_path(ctx.workspace_dir, raw_path)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, content).await?;

        Ok(json!({
            "saved": true,
            "path": raw_path,
            "bytes": content.len()
        }))
    }
}

struct SkillReadTool;

#[async_trait]
impl Tool for SkillReadTool {
    fn name(&self) -> &'static str {
        "skill_read"
    }

    fn description(&self) -> &'static str {
        "Load the full SKILL.md instructions and companion script list for an installed skill by name."
    }

    fn risk_level(&self) -> &'static str {
        "read_only"
    }

    fn default_policy(&self) -> ToolPolicy {
        ToolPolicy::AutoAllow
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name listed in Available Skills"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext<'_>) -> anyhow::Result<Value> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: name"))?;
        let detail = skills::get_skill(ctx.skills_dir, name).await?;
        Ok(serde_json::to_value(detail)?)
    }
}

struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "web_fetch"
    }

    fn description(&self) -> &'static str {
        "Fetch a public web page via HTTP GET and extract readable plain text."
    }

    fn risk_level(&self) -> &'static str {
        "read_only"
    }

    fn default_policy(&self) -> ToolPolicy {
        ToolPolicy::AutoAllow
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The http:// or https:// URL to fetch"
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Optional max character count to return (default 6000)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext<'_>) -> anyhow::Result<Value> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: url"))?;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(anyhow!("url must start with http:// or https://"));
        }
        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(6000) as usize;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("aiworker/0.1")
            .build()?;

        let resp = client.get(url).send().await.context("failed to fetch URL")?;
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        let plain = strip_html_to_text(&body);
        let excerpt: String = plain.chars().take(max_chars).collect();

        Ok(json!({
            "url": url,
            "status": status,
            "text": excerpt
        }))
    }
}

struct HttpRequestTool;

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &'static str {
        "http_request"
    }

    fn description(&self) -> &'static str {
        "Send a custom HTTP request (GET, POST, PUT, DELETE) to an external API or webhook."
    }

    fn risk_level(&self) -> &'static str {
        "external"
    }

    fn default_policy(&self) -> ToolPolicy {
        ToolPolicy::RequireApproval
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE"],
                    "description": "HTTP method"
                },
                "url": {
                    "type": "string",
                    "description": "Target URL"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers map"
                },
                "body": {
                    "description": "Optional JSON or text body"
                }
            },
            "required": ["method", "url"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext<'_>) -> anyhow::Result<Value> {
        let method_str = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        let method = Method::from_bytes(method_str.as_bytes())
            .map_err(|_| anyhow!("unsupported HTTP method: {method_str}"))?;
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: url"))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()?;

        let mut req = client.request(method, url);
        if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    req = req.header(k, val);
                }
            }
        }
        if let Some(body) = args.get("body") {
            if let Some(s) = body.as_str() {
                req = req.body(s.to_string());
            } else if !body.is_null() {
                req = req.json(body);
            }
        }

        let resp = req.send().await.context("HTTP request failed")?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let redacted = sandbox::redact_secrets(&text, ctx.secrets);
        let preview: String = redacted.chars().take(4000).collect();

        Ok(json!({
            "status": status,
            "body": preview
        }))
    }
}

struct PythonExecTool;

#[async_trait]
impl Tool for PythonExecTool {
    fn name(&self) -> &'static str {
        "python_exec"
    }

    fn description(&self) -> &'static str {
        "Execute a Python 3 script inside the isolated workspace sandbox (Docker if available, else restricted local)."
    }

    fn risk_level(&self) -> &'static str {
        "exec"
    }

    fn default_policy(&self) -> ToolPolicy {
        ToolPolicy::RequireApproval
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "Python 3 source code to run with working directory set to workspace."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (1..=120)."
                }
            },
            "required": ["code"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext<'_>) -> anyhow::Result<Value> {
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: code"))?;
        let timeout = args.get("timeout_secs").and_then(|v| v.as_u64());
        let output =
            sandbox::execute_python(code, ctx.workspace_dir, ctx.secrets, ctx.runtime, timeout)
                .await?;
        Ok(serde_json::to_value(output)?)
    }
}

struct ShellExecTool;

#[async_trait]
impl Tool for ShellExecTool {
    fn name(&self) -> &'static str {
        "shell_exec"
    }

    fn description(&self) -> &'static str {
        "Execute a Linux shell command (sh -c) inside the isolated workspace sandbox."
    }

    fn risk_level(&self) -> &'static str {
        "exec"
    }

    fn default_policy(&self) -> ToolPolicy {
        ToolPolicy::RequireApproval
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "POSIX shell command to run inside workspace."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (1..=120)."
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext<'_>) -> anyhow::Result<Value> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: command"))?;
        let timeout = args.get("timeout_secs").and_then(|v| v.as_u64());
        let output =
            sandbox::execute_shell(command, ctx.workspace_dir, ctx.secrets, ctx.runtime, timeout)
                .await?;
        Ok(serde_json::to_value(output)?)
    }
}

/// Lightweight HTML-to-text extractor stripping script/style blocks and HTML tags.
fn strip_html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len().min(8192));
    let mut in_tag = false;
    let mut skip_block = false;
    let mut tag_buf = String::new();
    let mut last_space = true;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
                let lower = tag_buf.trim().to_ascii_lowercase();
                if lower.starts_with("script") || lower.starts_with("style") {
                    skip_block = true;
                } else if lower.starts_with("/script") || lower.starts_with("/style") {
                    skip_block = false;
                }
                if !last_space {
                    out.push(' ');
                    last_space = true;
                }
            } else if tag_buf.len() < 32 {
                tag_buf.push(ch);
            }
            continue;
        }
        if skip_block {
            continue;
        }
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        let base = Path::new("/tmp/aiworker_test");
        assert!(resolve_safe_path(base, "../etc/passwd").is_err());
        assert!(resolve_safe_path(base, "notes/today.md").is_ok());
    }

    #[test]
    fn strips_html_scripts_and_tags() {
        let html = "<html><head><style>body{color:red}</style></head><body><h1>Hello</h1><script>alert(1)</script><p>World</p></body></html>";
        assert_eq!(strip_html_to_text(html), "Hello World");
    }
}
