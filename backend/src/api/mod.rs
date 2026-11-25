mod capabilities;
mod runs;
mod settings;
mod tasks;
mod workspace;

use std::path::PathBuf;

use axum::{
    Json, Router, middleware,
    routing::{delete, get, patch, post},
};
use serde_json::{Value, json};
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
};

use crate::{
    auth::{AuthConfig, require_auth},
    engine::AgentEngine,
};

#[derive(Clone)]
pub struct AppState {
    pub engine: AgentEngine,
}

pub fn build_router(state: AppState) -> Router {
    build_router_with_options(state, AuthConfig::default(), None)
}

pub fn build_router_with_options(
    state: AppState,
    auth: AuthConfig,
    static_dir: Option<PathBuf>,
) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut router = Router::new()
        .route("/api/health", get(health))
        // Tasks & Inbound Webhooks
        .route("/api/tasks", get(tasks::list_tasks).post(tasks::create_task))
        .route(
            "/api/tasks/{id}",
            get(tasks::get_task)
                .patch(tasks::update_task)
                .delete(tasks::delete_task),
        )
        .route("/api/tasks/{id}/run", post(tasks::trigger_task_run))
        .route("/api/hooks/{token}", post(tasks::trigger_inbound_webhook))
        // Runs, SSE & Approvals
        .route("/api/runs", get(runs::list_runs))
        .route("/api/runs/{id}", get(runs::get_run_detail))
        .route("/api/runs/{id}/events", get(runs::stream_run_events))
        .route("/api/runs/{id}/cancel", post(runs::cancel_run))
        .route("/api/events", get(runs::stream_all_events))
        .route("/api/approvals", get(runs::list_approvals))
        .route("/api/approvals/{id}/decide", post(runs::decide_approval))
        // Tools, Skills, Workers & Memories
        .route("/api/tools", get(capabilities::list_tools))
        .route(
            "/api/tools/{name}/policy",
            patch(capabilities::update_tool_policy),
        )
        .route(
            "/api/skills",
            get(capabilities::list_skills).post(capabilities::upsert_skill),
        )
        .route(
            "/api/skills/{name}",
            get(capabilities::get_skill).delete(capabilities::delete_skill),
        )
        .route(
            "/api/workers",
            get(capabilities::list_workers).post(capabilities::create_worker),
        )
        .route(
            "/api/workers/{id}",
            patch(capabilities::update_worker).delete(capabilities::delete_worker),
        )
        .route(
            "/api/memories",
            get(capabilities::list_memories).post(capabilities::upsert_memory),
        )
        .route("/api/memories/{id}", delete(capabilities::delete_memory))
        // Workspace Files
        .route(
            "/api/workspace/files",
            get(workspace::list_workspace_files).delete(workspace::delete_workspace_file),
        )
        .route("/api/workspace/upload", post(workspace::upload_workspace_file))
        .route(
            "/api/workspace/download",
            get(workspace::download_workspace_file),
        )
        // Secrets, Notifications, Backup & Settings
        .route(
            "/api/secrets",
            get(settings::list_secrets).post(settings::upsert_secret),
        )
        .route("/api/secrets/{id}", delete(settings::delete_secret))
        .route(
            "/api/notifications",
            get(settings::list_notifications).post(settings::create_notification),
        )
        .route(
            "/api/notifications/{id}",
            patch(settings::update_notification).delete(settings::delete_notification),
        )
        .route(
            "/api/notifications/{id}/test",
            post(settings::test_notification),
        )
        .route(
            "/api/backup/settings",
            get(settings::get_backup_settings).put(settings::put_backup_settings),
        )
        .route("/api/backup/export", get(settings::export_local_backup))
        .route("/api/backup/s3/push", post(settings::push_s3_backup))
        .route("/api/backup/s3/list", get(settings::list_s3_backups))
        .route("/api/backup/s3/restore", post(settings::restore_s3_backup))
        .route(
            "/api/settings/llm",
            get(settings::get_llm_settings).put(settings::put_llm_settings),
        )
        .route(
            "/api/settings/runtime",
            get(settings::get_runtime_settings).put(settings::put_runtime_settings),
        )
        .with_state(state);

    if let Some(dir) = static_dir {
        let index_file = dir.join("index.html");
        let serve = ServeDir::new(dir).not_found_service(ServeFile::new(index_file));
        router = router.fallback_service(serve);
    }

    router
        .layer(middleware::from_fn_with_state(auth, require_auth))
        .layer(cors)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "aiworker-backend" }))
}
