use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSettings {
    /// "auto" (Docker if available, else local restricted) | "docker" | "local"
    pub sandbox_mode: String,
    pub docker_image: String,
    /// "none" (no internet inside sandbox) | "bridge"
    pub docker_network: String,
    pub default_timeout_secs: u64,
    pub max_concurrent_runs: usize,
    /// 0 means unlimited
    pub daily_token_budget: i64,
    /// Number of days to retain completed/failed/cancelled runs (0 means keep forever)
    pub retention_days: i64,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            sandbox_mode: "auto".to_string(),
            docker_image: "python:3.12-slim".to_string(),
            docker_network: "none".to_string(),
            default_timeout_secs: 30,
            max_concurrent_runs: 4,
            daily_token_budget: 500_000,
            retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxOutput {
    pub backend: String, // "docker" | "local"
    pub exit_code: i32,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
}

const MAX_STREAM_CHARS: usize = 12_000;

pub async fn is_docker_available() -> bool {
    let res = tokio::time::timeout(
        Duration::from_secs(2),
        Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await;

    matches!(res, Ok(Ok(status)) if status.success())
}

pub async fn execute_python(
    code: &str,
    workspace_dir: &Path,
    secrets: &HashMap<String, String>,
    cfg: &RuntimeSettings,
    timeout_override: Option<u64>,
) -> anyhow::Result<SandboxOutput> {
    execute_command(
        &["python3", "-u", "-c", code],
        workspace_dir,
        secrets,
        cfg,
        timeout_override,
    )
    .await
}

pub async fn execute_shell(
    command: &str,
    workspace_dir: &Path,
    secrets: &HashMap<String, String>,
    cfg: &RuntimeSettings,
    timeout_override: Option<u64>,
) -> anyhow::Result<SandboxOutput> {
    execute_command(
        &["sh", "-c", command],
        workspace_dir,
        secrets,
        cfg,
        timeout_override,
    )
    .await
}

async fn execute_command(
    argv: &[&str],
    workspace_dir: &Path,
    secrets: &HashMap<String, String>,
    cfg: &RuntimeSettings,
    timeout_override: Option<u64>,
) -> anyhow::Result<SandboxOutput> {
    tokio::fs::create_dir_all(workspace_dir).await?;
    let timeout_secs = timeout_override
        .unwrap_or(cfg.default_timeout_secs)
        .clamp(1, 120);

    let use_docker = match cfg.sandbox_mode.as_str() {
        "docker" => {
            if !is_docker_available().await {
                return Err(anyhow!(
                    "sandbox_mode is set to 'docker', but Docker daemon is not reachable"
                ));
            }
            true
        }
        "local" => false,
        _ => is_docker_available().await,
    };

    if use_docker {
        run_in_docker(argv, workspace_dir, secrets, cfg, timeout_secs).await
    } else {
        run_in_local(argv, workspace_dir, secrets, timeout_secs).await
    }
}

async fn run_in_docker(
    argv: &[&str],
    workspace_dir: &Path,
    secrets: &HashMap<String, String>,
    cfg: &RuntimeSettings,
    timeout_secs: u64,
) -> anyhow::Result<SandboxOutput> {
    let abs_workspace = tokio::fs::canonicalize(workspace_dir)
        .await
        .unwrap_or_else(|_| workspace_dir.to_path_buf());

    let mut cmd = Command::new("docker");
    cmd.arg("run")
        .arg("--rm")
        .arg("--memory=256m")
        .arg("--cpus=1")
        .arg("--pids-limit=64")
        .arg(format!(
            "--network={}",
            if cfg.docker_network == "bridge" {
                "bridge"
            } else {
                "none"
            }
        ))
        .arg("-v")
        .arg(format!("{}:/workspace", abs_workspace.display()))
        .arg("-w")
        .arg("/workspace");

    for (k, v) in secrets {
        cmd.arg("-e").arg(format!("{k}={v}"));
    }

    cmd.arg(&cfg.docker_image);
    for arg in argv {
        cmd.arg(arg);
    }

    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    run_child_with_timeout(cmd, "docker", secrets, timeout_secs).await
}

async fn run_in_local(
    argv: &[&str],
    workspace_dir: &Path,
    secrets: &HashMap<String, String>,
    timeout_secs: u64,
) -> anyhow::Result<SandboxOutput> {
    let (program, rest) = argv
        .split_first()
        .ok_or_else(|| anyhow!("empty command"))?;

    let mut cmd = Command::new(program);
    cmd.args(rest)
        .current_dir(workspace_dir)
        .env_clear()
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_string()),
        )
        .env("HOME", workspace_dir)
        .env("LANG", "C.UTF-8")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // Inject only explicitly configured vault secrets (never host API keys)
    for (k, v) in secrets {
        cmd.env(k, v);
    }

    run_child_with_timeout(cmd, "local", secrets, timeout_secs).await
}

async fn run_child_with_timeout(
    mut cmd: Command,
    backend: &str,
    secrets: &HashMap<String, String>,
    timeout_secs: u64,
) -> anyhow::Result<SandboxOutput> {
    let child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn {backend} sandbox process"))?;

    match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout_raw = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_raw = String::from_utf8_lossy(&output.stderr).to_string();
            Ok(SandboxOutput {
                backend: backend.to_string(),
                exit_code: output.status.code().unwrap_or(-1),
                timed_out: false,
                stdout: truncate_stream(&redact_secrets(&stdout_raw, secrets)),
                stderr: truncate_stream(&redact_secrets(&stderr_raw, secrets)),
            })
        }
        Ok(Err(err)) => Err(anyhow!("sandbox execution failed: {err}")),
        Err(_) => Ok(SandboxOutput {
            backend: backend.to_string(),
            exit_code: -1,
            timed_out: true,
            stdout: String::new(),
            stderr: format!("Execution timed out after {timeout_secs}s and was terminated."),
        }),
    }
}

pub fn redact_secrets(input: &str, secrets: &HashMap<String, String>) -> String {
    let mut out = input.to_string();
    for (name, val) in secrets {
        if val.len() >= 4 && out.contains(val.as_str()) {
            out = out.replace(val.as_str(), &format!("[REDACTED:{name}]"));
        }
    }
    out
}

fn truncate_stream(s: &str) -> String {
    if s.chars().count() > MAX_STREAM_CHARS {
        let head: String = s.chars().take(MAX_STREAM_CHARS).collect();
        format!("{head}\n...[truncated]")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn executes_python_and_redacts_secrets_in_local_mode() {
        let dir = std::env::temp_dir().join(format!("aiworker_sbx_{}", uuid::Uuid::new_v4()));
        let mut secrets = HashMap::new();
        secrets.insert("MY_TOKEN".to_string(), "super_secret_12345".to_string());

        let cfg = RuntimeSettings {
            sandbox_mode: "local".to_string(),
            ..RuntimeSettings::default()
        };

        let out = execute_python(
            "import os; print('sum=', 2 + 3); print('tok=', os.environ.get('MY_TOKEN'))",
            &dir,
            &secrets,
            &cfg,
            Some(10),
        )
        .await
        .unwrap();

        assert_eq!(out.exit_code, 0);
        assert!(!out.timed_out);
        assert!(out.stdout.contains("sum= 5"));
        assert!(!out.stdout.contains("super_secret_12345"));
        assert!(out.stdout.contains("[REDACTED:MY_TOKEN]"));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn enforces_shell_timeout() {
        let dir = std::env::temp_dir().join(format!("aiworker_sbx_to_{}", uuid::Uuid::new_v4()));
        let cfg = RuntimeSettings {
            sandbox_mode: "local".to_string(),
            ..RuntimeSettings::default()
        };

        let out = execute_shell("sleep 5", &dir, &HashMap::new(), &cfg, Some(1))
            .await
            .unwrap();

        assert!(out.timed_out);
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
