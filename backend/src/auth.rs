use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Clone, Default)]
pub struct AuthConfig {
    pub token: Option<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let token = std::env::var("AIWORKER_AUTH_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Self { token }
    }
}

/// Optional single-user Bearer token guard.
/// When AIWORKER_AUTH_TOKEN is unset, all requests pass through for local dev convenience.
pub async fn require_auth(
    State(auth): State<AuthConfig>,
    req: Request,
    next: Next,
) -> Response {
    let Some(expected) = &auth.token else {
        return next.run(req).await;
    };

    let path = req.uri().path();
    // Allow non-API static assets, health probe, and token-authenticated inbound webhooks
    if !path.starts_with("/api/")
        || path == "/api/health"
        || path.starts_with("/api/hooks/")
    {
        return next.run(req).await;
    }

    // 1. Check Authorization: Bearer <token>
    if let Some(val) = req.headers().get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(bearer) = val.strip_prefix("Bearer ") {
            if bearer.trim() == expected {
                return next.run(req).await;
            }
        }
    }

    // 2. Fallback to ?token=<token> (needed by browser EventSource for SSE)
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("token=") {
                if val == expected {
                    return next.run(req).await;
                }
            }
        }
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "Unauthorized: missing or invalid AIWORKER_AUTH_TOKEN" })),
    )
        .into_response()
}
