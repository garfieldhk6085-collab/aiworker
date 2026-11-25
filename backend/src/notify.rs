use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    pub channel_type: String, // "webhook" | "feishu" | "slack" | "discord"
    pub target_url: String,
    pub events_json: String,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateNotificationRequest {
    pub name: String,
    #[serde(default = "default_channel_type")]
    pub channel_type: String,
    pub target_url: String,
    #[serde(default = "default_events")]
    pub events: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_channel_type() -> String {
    "webhook".to_string()
}

fn default_events() -> Vec<String> {
    vec![
        "approval_needed".to_string(),
        "completed".to_string(),
        "failed".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateNotificationRequest {
    pub name: Option<String>,
    pub channel_type: Option<String>,
    pub target_url: Option<String>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

/// Fire-and-forget asynchronous notification dispatcher for all subscribed channels.
pub fn dispatch_event(
    pool: SqlitePool,
    event_type: String,
    title: String,
    message: String,
    metadata: Value,
) {
    tokio::spawn(async move {
        let channels = match sqlx::query_as::<_, NotificationChannel>(
            "SELECT * FROM notifications WHERE enabled = 1",
        )
        .fetch_all(&pool)
        .await
        {
            Ok(list) => list,
            Err(err) => {
                tracing::warn!(error = %err, "failed to load notification channels");
                return;
            }
        };

        for ch in channels {
            let subscribed: Vec<String> =
                serde_json::from_str(&ch.events_json).unwrap_or_default();
            if !subscribed.is_empty() && !subscribed.iter().any(|e| e == &event_type) {
                continue;
            }
            if let Err(err) = send_to_channel(&ch, &event_type, &title, &message, &metadata).await {
                tracing::warn!(
                    channel_id = %ch.id,
                    channel_name = %ch.name,
                    error = %err,
                    "failed to send notification"
                );
            }
        }
    });
}

pub async fn send_to_channel(
    ch: &NotificationChannel,
    event_type: &str,
    title: &str,
    message: &str,
    metadata: &Value,
) -> anyhow::Result<()> {
    let payload = format_payload(&ch.channel_type, event_type, title, message, metadata);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp = client.post(&ch.target_url).json(&payload).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("notification webhook returned {status}: {body}");
    }
    Ok(())
}

fn format_payload(
    channel_type: &str,
    event_type: &str,
    title: &str,
    message: &str,
    metadata: &Value,
) -> Value {
    let text = format!("[aiworker · {event_type}] {title}\n{message}");
    match channel_type {
        "feishu" => json!({
            "msg_type": "text",
            "content": { "text": text }
        }),
        "slack" => json!({
            "text": text
        }),
        "discord" => json!({
            "content": text
        }),
        _ => json!({
            "source": "aiworker",
            "event": event_type,
            "title": title,
            "message": message,
            "metadata": metadata
        }),
    }
}
