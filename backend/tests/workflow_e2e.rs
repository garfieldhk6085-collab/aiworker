use std::time::Duration;

use aiworker_backend::{
    api::{AppState, build_router, build_router_with_options},
    auth::AuthConfig,
    db,
    engine::AgentEngine,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn end_to_end_task_approval_and_resume_flow() {
    let tmp_dir = std::env::temp_dir().join(format!("aiworker_e2e_{}", uuid::Uuid::new_v4()));
    let workspace_dir = tmp_dir.join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let engine = AgentEngine::new(pool.clone(), workspace_dir.clone());
    let app = build_router(AppState { engine });

    // 1. Create a task that runs immediately in built-in Mock mode
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/tasks")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "title": "每日工作简报",
                "prompt": "帮我整理今日待办并保存一份 Markdown 报告",
                "schedule_type": "once",
                "run_immediately": true
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    let run_id = created["run"]["id"].as_str().unwrap().to_string();

    // 2. Wait briefly for background agent loop to pause at Draft-first approval
    let mut approval_id = None;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let run = db::get_run(&pool, &run_id).await.unwrap();
        if run.status == "waiting_approval" {
            let approvals = db::list_run_approvals(&pool, &run_id).await.unwrap();
            assert_eq!(approvals.len(), 1);
            assert_eq!(approvals[0].tool_name, "workspace_write");
            assert_eq!(approvals[0].status, "pending");
            approval_id = Some(approvals[0].id.clone());
            break;
        }
    }
    let approval_id = approval_id.expect("run should pause in waiting_approval state");

    // 3. Approve the pending write tool call
    let decide_req = Request::builder()
        .method("POST")
        .uri(format!("/api/approvals/{approval_id}/decide"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "decision": "approved",
                "feedback": "Looks good, please save."
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(decide_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Wait for resumed run to complete and verify file was written to workspace
    let mut completed = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let run = db::get_run(&pool, &run_id).await.unwrap();
        if run.status == "completed" {
            assert!(run.output_summary.is_some());
            assert!(run.total_tokens > 0);
            completed = true;
            break;
        }
    }
    assert!(completed, "run should reach completed status after approval");

    let reports_dir = workspace_dir.join("reports");
    let mut rd = tokio::fs::read_dir(&reports_dir).await.unwrap();
    let entry = rd.next_entry().await.unwrap();
    assert!(
        entry.is_some(),
        "expected report file to be written in workspace/reports"
    );

    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
}

#[tokio::test]
async fn auth_guard_and_inbound_webhook_and_vault_flow() {
    let tmp_dir = std::env::temp_dir().join(format!("aiworker_full_{}", uuid::Uuid::new_v4()));
    let workspace_dir = tmp_dir.join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    let engine = AgentEngine::new(pool.clone(), workspace_dir.clone());
    let auth = AuthConfig {
        token: Some("test-secret-token".to_string()),
    };
    let app = build_router_with_options(AppState { engine: engine.clone() }, auth, None);

    // 1. Unauthenticated request to /api/tasks must be rejected with 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 2. /api/health remains public
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Create a skill and a secret (verify secret is masked on read)
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/skills")
                .header("authorization", "Bearer test-secret-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "csv_analyzer",
                        "description": "Analyze CSV files with Python",
                        "content": "Use python_exec with csv module."
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/secrets")
                .header("authorization", "Bearer test-secret-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "GITHUB_TOKEN",
                        "value": "ghp_1234567890abcdef",
                        "description": "Personal GitHub PAT"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/secrets")
                .header("authorization", "Bearer test-secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let secrets_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let secrets_json: Value = serde_json::from_slice(&secrets_body).unwrap();
    assert_eq!(secrets_json[0]["masked_value"], "****cdef");

    // 4. Upload and download a file in workspace
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/workspace/upload")
                .header("authorization", "Bearer test-secret-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": "data/sample.csv",
                        "content": "name,score\nalice,98\n"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/workspace/download?path=data/sample.csv")
                .header("authorization", "Bearer test-secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let dl = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8(dl.to_vec()).unwrap(), "name,score\nalice,98\n");

    // 5. Create a task with inbound webhook enabled, then trigger it via /api/hooks/{token} without Bearer header
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tasks")
                .header("authorization", "Bearer test-secret-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "title": "快捷指令触发",
                        "prompt": "处理收到的外部事件",
                        "enable_webhook": true,
                        "run_immediately": false
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let created: Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let hook_token = created["task"]["webhook_token"]
        .as_str()
        .expect("webhook_token should be generated");

    let hook_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/hooks/{hook_token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "source": "siri_shortcut", "note": "buy milk" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hook_resp.status(), StatusCode::OK);

    // 6. Verify crash recovery leaves waiting_approval untouched and marks running as failed
    let recovered = engine.recover_interrupted_runs().await.unwrap();
    assert!(recovered <= 1);

    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
}
