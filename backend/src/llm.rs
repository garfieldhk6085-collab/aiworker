use std::sync::Arc;

use anyhow::anyhow;
use rig_core::{
    completion::{CompletionRequest, CompletionResponse, ToolDefinition},
    message::{
        AssistantContent, AssistantMessage, CallId, Message, Text, ToolCall as RigToolCall,
        ToolFunction as RigToolFunction, ToolName, ToolResult, ToolResultContent, UserContent,
    },
    providers::{
        anthropic::wire::AnthropicConfig,
        gemini::GeminiConfig,
        ollama::OllamaConfig,
        openai::wire::OpenAIConfig,
    },
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::models::{ChatMessage, LlmSettings, ToolCall, ToolCallFunction};

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub total_tokens: i64,
}

/// Dispatch chat completion through `rig-core` (OpenAI / Anthropic / Gemini / Ollama)
/// or the built-in deterministic Mock provider when no API key is configured.
pub async fn complete_chat(
    settings: &LlmSettings,
    messages: &[ChatMessage],
    tools: &[Value],
) -> anyhow::Result<LlmResponse> {
    let provider = settings.provider.as_str();
    if provider == "mock"
        || (provider != "ollama" && settings.api_key.trim().is_empty())
    {
        return Ok(mock_complete(messages));
    }

    let mut last_err = None;
    for attempt in 0..3u32 {
        let request = build_rig_request(settings, messages, tools)?;
        match call_provider_once(settings, request).await {
            Ok(resp) => return Ok(convert_rig_response(resp)),
            Err(err) => {
                let msg = err.to_string();
                let retryable = msg.contains("429")
                    || msg.contains("500")
                    || msg.contains("502")
                    || msg.contains("503")
                    || msg.contains("504")
                    || msg.to_ascii_lowercase().contains("timeout")
                    || msg.to_ascii_lowercase().contains("connection");
                last_err = Some(err);
                if !retryable || attempt == 2 {
                    break;
                }
                let backoff_ms = 500u64 * (1u64 << attempt);
                tracing::warn!(attempt = attempt + 1, backoff_ms, error = %msg, "retrying transient LLM error");
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("LLM call failed")))
}

async fn call_provider_once(
    settings: &LlmSettings,
    request: CompletionRequest,
) -> anyhow::Result<CompletionResponse> {
    let response = match settings.provider.as_str() {
        "anthropic" => {
            let mut cfg = AnthropicConfig::new(settings.api_key.trim());
            if !settings.base_url.trim().is_empty() {
                cfg = cfg.with_base_url(settings.base_url.trim());
            }
            cfg.client()
                .completion(&settings.model)
                .call(request)
                .await
                .map_err(|e| anyhow!("Anthropic (rig-core) error: {e}"))?
        }
        "gemini" => {
            let mut cfg = GeminiConfig::new(settings.api_key.trim());
            if !settings.base_url.trim().is_empty() {
                cfg = cfg.with_base_url(settings.base_url.trim());
            }
            cfg.client()
                .completion(&settings.model)
                .call(request)
                .await
                .map_err(|e| anyhow!("Gemini (rig-core) error: {e}"))?
        }
        "ollama" => {
            let mut cfg = OllamaConfig::new();
            if !settings.base_url.trim().is_empty() {
                cfg = cfg.with_base_url(settings.base_url.trim());
            }
            if !settings.api_key.trim().is_empty() {
                cfg = cfg.with_api_key(settings.api_key.trim());
            }
            cfg.client()
                .completion(&settings.model)
                .call(request)
                .await
                .map_err(|e| anyhow!("Ollama (rig-core) error: {e}"))?
        }
        // "openai" | "openai_compatible" | "deepseek" | other OpenAI-shaped providers
        _ => {
            let mut cfg = OpenAIConfig::new(settings.api_key.trim());
            if !settings.base_url.trim().is_empty() {
                cfg = cfg.with_base_url(settings.base_url.trim());
            }
            cfg.client()
                .chat(&settings.model)
                .call(request)
                .await
                .map_err(|e| anyhow!("OpenAI-compatible (rig-core) error: {e}"))?
        }
    };

    Ok(response)
}

fn build_rig_request(
    settings: &LlmSettings,
    messages: &[ChatMessage],
    tools: &[Value],
) -> anyhow::Result<CompletionRequest> {
    let rig_messages = to_rig_messages(messages);
    let mut iter = rig_messages.into_iter();
    let first = iter
        .next()
        .ok_or_else(|| anyhow!("cannot send empty chat history to LLM"))?;

    let mut req = CompletionRequest::new(first);
    req.chat_history = std::iter::once(req.chat_history.remove(0))
        .chain(iter.map(Arc::new))
        .collect();
    req.temperature = Some(f64::from(settings.temperature));

    let mut rig_tools = Vec::with_capacity(tools.len());
    for tool_val in tools {
        let Some(func) = tool_val.get("function") else {
            continue;
        };
        let Some(name_str) = func.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(tool_name) = ToolName::new(name_str) else {
            continue;
        };
        let description = func
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let parameters = func
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
        rig_tools.push(Arc::new(ToolDefinition::new(tool_name, description, parameters)));
    }
    req.tools = rig_tools;

    Ok(req)
}

fn to_rig_messages(messages: &[ChatMessage]) -> Vec<Message> {
    let mut out = Vec::with_capacity(messages.len());

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                if let Some(content) = &msg.content {
                    out.push(Message::System {
                        content: content.clone(),
                    });
                }
            }
            "user" => {
                let text = msg.content.clone().unwrap_or_default();
                out.push(Message::User {
                    content: vec![UserContent::Text(Text::new(text))],
                });
            }
            "assistant" => {
                let mut blocks = Vec::new();
                if let Some(text) = &msg.content {
                    if !text.is_empty() {
                        blocks.push(AssistantContent::Text(Text::new(text.clone())));
                    }
                }
                if let Some(calls) = &msg.tool_calls {
                    for call in calls {
                        if let Ok(tool_name) = ToolName::new(&call.function.name) {
                            let args: Value = serde_json::from_str(&call.function.arguments)
                                .unwrap_or_else(|_| json!({}));
                            blocks.push(AssistantContent::ToolCall(RigToolCall::new(
                                CallId::from_wire(call.id.clone()),
                                RigToolFunction::new(tool_name, args),
                            )));
                        }
                    }
                }
                if !blocks.is_empty() {
                    out.push(Message::Assistant(AssistantMessage::new(blocks)));
                }
            }
            "tool" => {
                let call_id = msg
                    .tool_call_id
                    .clone()
                    .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
                let tool_name = msg
                    .name
                    .as_deref()
                    .and_then(|n| ToolName::new(n).ok())
                    .unwrap_or_else(|| ToolName::new("tool").expect("non-empty tool name"));
                let content_text = msg.content.clone().unwrap_or_default();
                let result = ToolResult {
                    call: CallId::from_wire(call_id),
                    name: tool_name,
                    content: vec![ToolResultContent::Text(Text::new(content_text))],
                    is_error: false,
                };
                out.push(Message::User {
                    content: vec![UserContent::ToolResult(result)],
                });
            }
            _ => {}
        }
    }

    out
}

fn convert_rig_response(resp: CompletionResponse) -> LlmResponse {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in resp.choice {
        match block {
            AssistantContent::Text(t) => {
                if !t.text.is_empty() {
                    text_parts.push(t.text);
                }
            }
            AssistantContent::ToolCall(call) => {
                tool_calls.push(ToolCall {
                    id: call.id.wire().into_owned(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction {
                        name: call.function.name.as_str().to_string(),
                        arguments: Value::Object(call.function.arguments).to_string(),
                    },
                });
            }
            _ => {}
        }
    }

    let content = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts.join("\n"))
    };

    let total_tokens = resp
        .usage
        .total_tokens
        .or_else(|| {
            match (resp.usage.input_tokens, resp.usage.output_tokens) {
                (Some(i), Some(o)) => Some(i + o),
                (Some(i), None) => Some(i),
                (None, Some(o)) => Some(o),
                (None, None) => None,
            }
        })
        .unwrap_or(0) as i64;

    LlmResponse {
        content,
        tool_calls,
        total_tokens,
    }
}

/// Deterministic built-in Mock LLM so users can test the full workflow
/// (read-only tool -> write tool requiring approval -> resume -> summary) without an API key.
fn mock_complete(messages: &[ChatMessage]) -> LlmResponse {
    let tool_results_count = messages.iter().filter(|m| m.role == "tool").count();
    let user_prompt = messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_deref())
        .unwrap_or("整理今日任务简报");

    match tool_results_count {
        0 => LlmResponse {
            content: Some(format!(
                "已收到任务目标：「{user_prompt}」。先检查本地工作区中已有的笔记和文件上下文。"
            )),
            tool_calls: vec![ToolCall {
                id: format!("call_{}", Uuid::new_v4().simple()),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: "workspace_read".to_string(),
                    arguments: json!({ "path": "." }).to_string(),
                },
            }],
            total_tokens: 185,
        },
        1 => {
            let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
            let filename = format!("reports/brief-{date}.md");
            let draft_content = format!(
                "# AI Worker 任务交付报告 ({date})\n\n## 目标\n{user_prompt}\n\n## 执行摘要\n- 已扫描本地工作区上下文\n- 已按个人偏好整理结构化行动建议\n\n## 建议下一步\n1. 确认关键优先级事项\n2. 如需定期自动生成，可将此任务切换为 Cron 定时调度\n"
            );
            LlmResponse {
                content: Some(
                    "已读取工作区内容。现在准备将整理好的结构化 Markdown 报告保存到本地工作区（写入操作将进入待审批收件箱等待你的确认）。"
                        .to_string(),
                ),
                tool_calls: vec![ToolCall {
                    id: format!("call_{}", Uuid::new_v4().simple()),
                    call_type: "function".to_string(),
                    function: ToolCallFunction {
                        name: "workspace_write".to_string(),
                        arguments: json!({
                            "path": filename,
                            "content": draft_content
                        })
                        .to_string(),
                    },
                }],
                total_tokens: 260,
            }
        }
        _ => {
            let last_tool_msg = messages
                .iter()
                .rev()
                .find(|m| m.role == "tool")
                .and_then(|m| m.content.as_deref())
                .unwrap_or("");

            let summary = if last_tool_msg.contains("\"rejected\"") {
                format!(
                    "已根据你的审批意见取消写入文件操作。\n\n以下为任务「{user_prompt}」的纯文本整理结果，未对本地文件做出修改。\n（当前运行在内置 Mock 演示模式，可在 Settings 页面通过 rig-core 接入 OpenAI / Anthropic / Gemini / DeepSeek / Ollama。）"
                )
            } else {
                format!(
                    "任务「{user_prompt}」已完成！\n\n- 已检查本地工作区上下文\n- 你的审批已通过，报告已成功写入本地工作区\n\n（当前运行在内置 Mock 演示模式，可在 Settings 页面通过 rig-core 接入 OpenAI / Anthropic / Gemini / DeepSeek / Ollama。）"
                )
            };

            LlmResponse {
                content: Some(summary),
                tool_calls: Vec::new(),
                total_tokens: 140,
            }
        }
    }
}
