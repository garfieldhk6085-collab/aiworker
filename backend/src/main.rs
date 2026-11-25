use std::{net::SocketAddr, path::PathBuf};

use aiworker_backend::{
    api::{AppState, build_router_with_options},
    auth::AuthConfig,
    db,
    engine::AgentEngine,
    scheduler::spawn_scheduler,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aiworker_backend=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let data_dir =
        PathBuf::from(std::env::var("AIWORKER_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let workspace_dir = PathBuf::from(
        std::env::var("AIWORKER_WORKSPACE_DIR").unwrap_or_else(|_| "./workspace".into()),
    );
    tokio::fs::create_dir_all(&data_dir).await?;
    tokio::fs::create_dir_all(&workspace_dir).await?;
    tokio::fs::create_dir_all(workspace_dir.join("skills")).await?;

    let db_path = data_dir.join("aiworker.db");
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| format!("sqlite://{}", db_path.display()));

    let pool = db::init_pool(&database_url).await?;
    let runtime = db::get_runtime_settings(&pool).await.unwrap_or_default();
    let engine = AgentEngine::new_with_dirs(
        pool,
        data_dir.clone(),
        workspace_dir.clone(),
        runtime.max_concurrent_runs,
    );
    let _ = engine.recover_interrupted_runs().await;
    spawn_scheduler(engine.clone());

    let auth = AuthConfig::from_env();
    let static_dir = std::env::var("AIWORKER_STATIC_DIR")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .or_else(|| {
            let default_dist = PathBuf::from("../frontend/dist");
            if default_dist.exists() {
                Some(default_dist)
            } else {
                None
            }
        });

    let app = build_router_with_options(AppState { engine }, auth.clone(), static_dir.clone());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!(
        "aiworker backend listening on http://{} (workspace: {}, auth: {}, static_ui: {})",
        addr,
        workspace_dir.display(),
        if auth.token.is_some() { "enabled" } else { "disabled" },
        static_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
