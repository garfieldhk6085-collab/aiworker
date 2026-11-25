use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::AppState,
    db::{self, now_iso},
    error::{AppError, AppResult},
    models::{
        CreateWorkerRequest, MemoryItem, ToolInfo, UpdateToolPolicyRequest, UpdateWorkerRequest,
        UpsertMemoryRequest, Worker,
    },
    skills::{self, SkillDetail, SkillSummary, UpsertSkillRequest},
};

pub async fn list_tools(State(state): State<AppState>) -> AppResult<Json<Vec<ToolInfo>>> {
    let policies = db::get_tool_policies(&state.engine.pool).await?;
    Ok(Json(state.engine.tools.list_info(&policies)))
}

pub async fn update_tool_policy(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<UpdateToolPolicyRequest>,
) -> AppResult<Json<Value>> {
    if state.engine.tools.get(&name).is_none() {
        return Err(AppError::NotFound(format!("tool '{name}' not found")));
    }
    db::save_tool_policy(&state.engine.pool, &name, req.policy).await?;
    Ok(Json(json!({ "name": name, "policy": req.policy })))
}

pub async fn list_skills(State(state): State<AppState>) -> AppResult<Json<Vec<SkillSummary>>> {
    let items = skills::list_skills(&state.engine.skills_dir)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(items))
}

pub async fn get_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<SkillDetail>> {
    let detail = skills::get_skill(&state.engine.skills_dir, &name)
        .await
        .map_err(|e| AppError::NotFound(e.to_string()))?;
    Ok(Json(detail))
}

pub async fn upsert_skill(
    State(state): State<AppState>,
    Json(req): Json<UpsertSkillRequest>,
) -> AppResult<Json<SkillDetail>> {
    let detail = skills::upsert_skill(&state.engine.skills_dir, &req)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(detail))
}

pub async fn delete_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<Value>> {
    skills::delete_skill(&state.engine.skills_dir, &name)
        .await
        .map_err(|e| AppError::NotFound(e.to_string()))?;
    Ok(Json(json!({ "deleted": true, "name": name })))
}

pub async fn list_workers(State(state): State<AppState>) -> AppResult<Json<Vec<Worker>>> {
    Ok(Json(db::list_workers(&state.engine.pool).await?))
}

pub async fn create_worker(
    State(state): State<AppState>,
    Json(req): Json<CreateWorkerRequest>,
) -> AppResult<Json<Worker>> {
    let name = req.name.trim().to_string();
    if name.is_empty() || req.system_prompt.trim().is_empty() {
        return Err(AppError::BadRequest(
            "name and system_prompt are required".to_string(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let allowed_tools_json = req
        .allowed_tools
        .map(|list| serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()));

    sqlx::query(
        "INSERT INTO workers (id, name, description, system_prompt, provider, model, base_url, allowed_tools_json, max_steps, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&req.description)
    .bind(&req.system_prompt)
    .bind(&req.provider)
    .bind(&req.model)
    .bind(&req.base_url)
    .bind(&allowed_tools_json)
    .bind(req.max_steps.map(|s| s.clamp(2, 30)))
    .bind(&now)
    .bind(&now)
    .execute(&state.engine.pool)
    .await?;

    Ok(Json(db::get_worker(&state.engine.pool, &id).await?))
}

pub async fn update_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkerRequest>,
) -> AppResult<Json<Worker>> {
    let mut w = db::get_worker(&state.engine.pool, &id).await?;
    if let Some(name) = req.name {
        w.name = name;
    }
    if let Some(desc) = req.description {
        w.description = desc;
    }
    if let Some(sp) = req.system_prompt {
        w.system_prompt = sp;
    }
    if let Some(p) = req.provider {
        w.provider = if p.trim().is_empty() { None } else { Some(p) };
    }
    if let Some(m) = req.model {
        w.model = if m.trim().is_empty() { None } else { Some(m) };
    }
    if let Some(u) = req.base_url {
        w.base_url = if u.trim().is_empty() { None } else { Some(u) };
    }
    if let Some(allowed) = req.allowed_tools {
        w.allowed_tools_json =
            Some(serde_json::to_string(&allowed).unwrap_or_else(|_| "[]".to_string()));
    }
    if let Some(ms) = req.max_steps {
        w.max_steps = Some(ms.clamp(2, 30));
    }
    w.updated_at = now_iso();

    sqlx::query(
        "UPDATE workers SET name = ?, description = ?, system_prompt = ?, provider = ?, model = ?, base_url = ?, allowed_tools_json = ?, max_steps = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&w.name)
    .bind(&w.description)
    .bind(&w.system_prompt)
    .bind(&w.provider)
    .bind(&w.model)
    .bind(&w.base_url)
    .bind(&w.allowed_tools_json)
    .bind(w.max_steps)
    .bind(&w.updated_at)
    .bind(&w.id)
    .execute(&state.engine.pool)
    .await?;

    Ok(Json(w))
}

pub async fn delete_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let res = sqlx::query("DELETE FROM workers WHERE id = ?")
        .bind(&id)
        .execute(&state.engine.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("worker {id} not found")));
    }
    Ok(Json(json!({ "deleted": true, "id": id })))
}

pub async fn list_memories(State(state): State<AppState>) -> AppResult<Json<Vec<MemoryItem>>> {
    Ok(Json(db::list_memories(&state.engine.pool).await?))
}

pub async fn upsert_memory(
    State(state): State<AppState>,
    Json(req): Json<UpsertMemoryRequest>,
) -> AppResult<Json<MemoryItem>> {
    let key = req.key.trim().to_string();
    let content = req.content.trim().to_string();
    if key.is_empty() || content.is_empty() {
        return Err(AppError::BadRequest(
            "key and content are required".to_string(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let updated_at = now_iso();
    sqlx::query(
        "INSERT INTO memories (id, key, content, category, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET content = excluded.content, category = excluded.category, updated_at = excluded.updated_at",
    )
    .bind(&id)
    .bind(&key)
    .bind(&content)
    .bind(&req.category)
    .bind(&updated_at)
    .execute(&state.engine.pool)
    .await?;

    let item = sqlx::query_as::<_, MemoryItem>("SELECT * FROM memories WHERE key = ?")
        .bind(&key)
        .fetch_one(&state.engine.pool)
        .await?;
    Ok(Json(item))
}

pub async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let res = sqlx::query("DELETE FROM memories WHERE id = ?")
        .bind(&id)
        .execute(&state.engine.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("memory {id} not found")));
    }
    Ok(Json(json!({ "deleted": true, "id": id })))
}
