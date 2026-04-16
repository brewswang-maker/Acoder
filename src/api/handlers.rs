//! API Handlers
//!
//! REST API endpoint implementations.
//! Each handler: auth check → rate limit check → service call → ApiResponse<T>

use std::sync::Arc;
use axum::{
    extract::{Path, Query, Extension, State},
    Json, Router,
    routing::{get, post},
    response::IntoResponse,
    http::StatusCode,
};
use tokio::sync::RwLock;
use chrono::Utc;

use crate::api::models::*;
use crate::api::rate_limiter::RateLimiter;
use crate::api::middleware::AuthenticatedUser;

// ── Shared In-Memory State ────────────────────────────────────────────────

/// In-memory task storage (production would use a DB)
#[derive(Clone, Default)]
pub struct TaskStore(pub Arc<RwLock<std::collections::HashMap<String, crate::api::models::Task>>>);

/// Application state for API routes
#[derive(Clone)]
pub struct AppState {
    pub task_store: TaskStore,
    pub rate_limiter: Arc<RateLimiter>,
    pub start_time: std::time::Instant,
}

impl AppState {
    pub fn new(rate_limiter: Arc<RateLimiter>) -> Self {
        Self {
            task_store: TaskStore::default(),
            rate_limiter,
            start_time: std::time::Instant::now(),
        }
    }
}

// ── Health ────────────────────────────────────────────────────────────────

pub async fn health() -> Json<ApiResponse<HealthResponse>> {
    let uptime = std::time::Instant::now()
        .elapsed()
        .as_secs();

    Json(ApiResponse::success(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_secs: uptime,
    }))
}

// ── Tasks ────────────────────────────────────────────────────────────────

pub async fn create_task(
    Extension(user): Extension<AuthenticatedUser>,
    Extension(state): Extension<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    let task_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();

    let task = crate::api::models::Task {
        id: task_id.clone(),
        description: req.description.clone(),
        status: crate::api::models::TaskStatus::Pending,
        agent_id: None,
        agent_type: req.agent_type.clone(),
        priority: req.priority.unwrap_or(TaskPriority::Medium),
        created_at: now,
        updated_at: now,
    };

    // Store the task
    {
        let mut store = state.task_store.0.write().await;
        store.insert(task_id.clone(), task.clone());
    }

    tracing::info!("[API] User {} created task {}", user.user_id, task_id);

    let response: ApiResponse<TaskResponse> =
        ApiResponse::success(task.into());

    (StatusCode::CREATED, Json(response))
}

pub async fn get_task(
    Path(id): Path<String>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    use axum::response::Response;

    let store = state.task_store.0.read().await;

    match store.get(&id) {
        Some(task) => {
            let response: ApiResponse<TaskResponse> =
                ApiResponse::success(task.clone().into());
            (StatusCode::OK, Json(response)).into_response()
        }
        None => {
            let response: ApiResponse<()> =
                ApiResponse::error("NOT_FOUND", format!("Task {} not found", id));
            (StatusCode::NOT_FOUND, Json(response)).into_response()
        }
    }
}

pub async fn list_tasks(
    Extension(state): Extension<AppState>,
    Query(params): Query<ListTasksQuery>,
) -> impl IntoResponse {
    let store = state.task_store.0.read().await;

    let mut tasks: Vec<TaskResponse> = store
        .values()
        .filter(|t| {
            params.status.as_ref().map_or(true, |s| &t.status == s)
        })
        .cloned()
        .map(|t| t.into())
        .collect();

    // Sort by created_at desc
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(50);
    let total = tasks.len();
    tasks = tasks.into_iter().skip(offset).take(limit).collect();

    #[derive(serde::Serialize)]
    struct ListData {
        tasks: Vec<TaskResponse>,
        total: usize,
        offset: usize,
        limit: usize,
    }

    let response: ApiResponse<ListData> =
        ApiResponse::success(ListData { tasks, total, offset, limit });

    (StatusCode::OK, Json(response))
}

pub async fn cancel_task(
    Path(id): Path<String>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    use axum::response::Response;

    let mut store = state.task_store.0.write().await;

    match store.get_mut(&id) {
        Some(task) => {
            if task.status == TaskStatus::Pending || task.status == TaskStatus::Running {
                task.status = TaskStatus::Cancelled;
                task.updated_at = Utc::now();
                let response: ApiResponse<TaskResponse> =
                    ApiResponse::success(task.clone().into());
                (StatusCode::OK, Json(response)).into_response()
            } else {
                let response: ApiResponse<()> =
                    ApiResponse::error("INVALID_STATE", "Task cannot be cancelled in current state");
                (StatusCode::CONFLICT, Json(response)).into_response()
            }
        }
        None => {
            let response: ApiResponse<()> =
                ApiResponse::error("NOT_FOUND", format!("Task {} not found", id));
            (StatusCode::NOT_FOUND, Json(response)).into_response()
        }
    }
}

// ── Agents ───────────────────────────────────────────────────────────────

pub async fn list_agents() -> impl IntoResponse {
    // Use existing ExpertRegistry from agents module
    let registry = crate::agents::ExpertRegistry::new();
    let agents: Vec<AgentResponse> = registry
        .all()
        .iter()
        .map(|e| AgentResponse {
            id: e.id.clone(),
            name: e.name.clone(),
            role: e.expert_type.category().to_string(),
            status: AgentStatus::Idle,
            current_task: None,
            success_rate: 0.0,
        })
        .collect();

    let response: ApiResponse<Vec<AgentResponse>> =
        ApiResponse::success(agents);

    (StatusCode::OK, Json(response))
}

pub async fn create_agent(
    Json(req): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    let agent_id = uuid::Uuid::new_v4().to_string();

    let response: ApiResponse<AgentResponse> =
        ApiResponse::success(AgentResponse {
            id: agent_id,
            name: req.name,
            role: req.role,
            status: AgentStatus::Idle,
            current_task: None,
            success_rate: 0.0,
        });

    (StatusCode::CREATED, Json(response))
}

// ── Skills ───────────────────────────────────────────────────────────────

pub async fn list_skills() -> impl IntoResponse {
    let response: ApiResponse<Vec<SkillResponse>> =
        ApiResponse::success(vec![
            SkillResponse {
                id: "skill-registry".into(),
                name: "Skill Registry".into(),
                version: "1.0.0".into(),
                description: "Skill management system".into(),
                success_rate: 0.85,
                utility_score: 0.9,
            }
        ]);

    (StatusCode::OK, Json(response))
}

pub async fn evolve_skill(
    Json(req): Json<EvolveSkillRequest>,
) -> impl IntoResponse {
    tracing::info!("[API] Skill evolution triggered for: {}", req.name);

    let response: ApiResponse<EvolveSkillResponse> =
        ApiResponse::success(EvolveSkillResponse {
            skill: req.name,
            status: "evolution_started".into(),
            generation: 1,
        });

    (StatusCode::ACCEPTED, Json(response))
}

// ── Memory ────────────────────────────────────────────────────────────────

pub async fn get_session_memory(
    Extension(user): Extension<AuthenticatedUser>,
) -> impl IntoResponse {
    let response: ApiResponse<SessionMemoryResponse> =
        ApiResponse::success(SessionMemoryResponse {
            session_id: user.user_id.clone(),
            messages: vec![],
            summary: None,
        });

    (StatusCode::OK, Json(response))
}

// ── Billing ───────────────────────────────────────────────────────────────

pub async fn get_usage(
    Extension(user): Extension<AuthenticatedUser>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    let stats = state.rate_limiter.get_usage(&user.user_id).await;

    let response: ApiResponse<UsageStats> = ApiResponse::success(UsageStats {
        user_id: stats.user_id,
        period_start: stats.period_start,
        period_end: stats.period_end,
        total_requests: stats.total_requests,
        total_tokens: stats.total_tokens,
        prompt_tokens: stats.prompt_tokens,
        completion_tokens: stats.completion_tokens,
        cost_estimate: stats.cost_estimate,
        quota_limit: stats.quota_limit,
        quota_used: stats.quota_used,
        quota_remaining: stats.quota_remaining,
    });

    (StatusCode::OK, Json(response))
}

// ── Router Builder ────────────────────────────────────────────────────────

pub fn create_api_router(state: AppState) -> Router {
    Router::new()
        .route("/tasks", post(create_task).get(list_tasks))
        .route("/tasks/:id", get(get_task))
        .route("/tasks/:id/cancel", post(cancel_task))
        .route("/agents", get(list_agents).post(create_agent))
        .route("/skills", get(list_skills))
        .route("/skills/evolve", post(evolve_skill))
        .route("/memory/session", get(get_session_memory))
        .route("/billing/usage", get(get_usage))
        .route("/health", get(health))
        .with_state(state)
}
