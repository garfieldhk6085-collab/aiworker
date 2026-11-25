use std::convert::Infallible;

use axum::{
    Json,
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Deserialize;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};

use crate::{
    api::AppState,
    db,
    error::AppResult,
    models::{ApprovalWithContext, DecideApprovalRequest, RunDetailResponse, TaskRun, TaskRunWithTask},
};

#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
}

pub async fn list_runs(
    State(state): State<AppState>,
    Query(query): Query<RunsQuery>,
) -> AppResult<Json<Vec<TaskRunWithTask>>> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(db::list_runs(&state.engine.pool, limit).await?))
}

pub async fn get_run_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<RunDetailResponse>> {
    let run = db::get_run(&state.engine.pool, &id).await?;
    let task = db::get_task(&state.engine.pool, &run.task_id).await?;
    let steps = db::list_run_steps(&state.engine.pool, &id).await?;
    let approvals = db::list_run_approvals(&state.engine.pool, &id).await?;
    Ok(Json(RunDetailResponse {
        run,
        task,
        steps,
        approvals,
    }))
}

pub async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<TaskRun>> {
    Ok(Json(state.engine.cancel_run(&id).await?))
}

pub async fn stream_run_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.engine.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |msg| match msg {
        Ok(ev) if ev.run_id == id => {
            let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
            Some(Ok(Event::default().event(ev.event_type).data(data)))
        }
        _ => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub async fn stream_all_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.engine.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(ev) => {
            let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
            Some(Ok(Event::default().event(ev.event_type).data(data)))
        }
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Debug, Deserialize)]
pub struct ApprovalsQuery {
    pub status: Option<String>,
}

pub async fn list_approvals(
    State(state): State<AppState>,
    Query(query): Query<ApprovalsQuery>,
) -> AppResult<Json<Vec<ApprovalWithContext>>> {
    Ok(Json(
        db::list_approvals(&state.engine.pool, query.status.as_deref()).await?,
    ))
}

pub async fn decide_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<DecideApprovalRequest>,
) -> AppResult<Json<TaskRun>> {
    Ok(Json(
        state
            .engine
            .decide_approval(&id, &req.decision, req.feedback)
            .await?,
    ))
}
