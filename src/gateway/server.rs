//! HTTP Server

use axum::{
    routing::{get, post},
    Router, Json,
    extract::{Path, Extension},
    http::StatusCode,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use serde::{Deserialize, Serialize};
use crate::config::Config;
use crate::api;

/// Shared server state injected as an extension
#[derive(Clone)]
struct ServerState {
    config: Config,
    workdir: std::path::PathBuf,
}

pub async fn run(listener: TcpListener, config: Config, workdir: std::path::PathBuf) -> anyhow::Result<()> {
    let state = ServerState { config, workdir };
    let api_routes = api::routes::create_routes();

    let app = Router::new()
        .route("/", get(|| async { "Acode v0.1.0" }))
        .route("/api/v1/task", post(task_handler))
        .route("/api/v1/task/:id", get(task_status_handler))
        .route("/api/v1/analyze", post(analyze_handler))
        // Mount REST API v1 (auth + rate limiting middleware built-in)
        .nest("/api/v1", api_routes)
        // Inject server state as extension for legacy handlers
        .layer(Extension(state));

    tracing::info!("Gateway 服务启动于 {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Deserialize)]
struct TaskRequest {
    task: String,
    model: Option<String>,
}

#[derive(Serialize)]
struct TaskResponse {
    task_id: String,
    status: String,
}

async fn task_handler(
    Extension(state): Extension<ServerState>,
    Json(req): Json<TaskRequest>,
) -> (StatusCode, Json<TaskResponse>) {
    let task_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("收到任务: {} -> {}", task_id, req.task);
    (StatusCode::ACCEPTED, Json(TaskResponse {
        task_id,
        status: "queued".into(),
    }))
}

async fn task_status_handler(
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "task_id": id, "status": "running" }))
}

async fn analyze_handler(Json(_payload): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "report": "分析完成", "files": 42 }))
}
