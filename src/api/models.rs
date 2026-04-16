//! API Data Models
//!
//! REST API 请求/响应数据结构

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Task API ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub description: String,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub priority: Option<TaskPriority>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Medium
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskResponse {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Internal task storage
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub priority: TaskPriority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Task> for TaskResponse {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            description: t.description,
            status: t.status,
            agent_id: t.agent_id,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

// ── Agent API ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub role: String,
    pub status: AgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    pub success_rate: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "working")]
    Working,
    #[serde(rename = "blocked")]
    Blocked,
    #[serde(rename = "done")]
    Done,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub expert_type: Option<String>,
}

// ── Skill API ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillResponse {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub success_rate: f64,
    pub utility_score: f64,
}

#[derive(Debug, Deserialize)]
pub struct EvolveSkillRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct EvolveSkillResponse {
    pub skill: String,
    pub status: String,
    pub generation: usize,
}

// ── Memory API ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SessionMemoryResponse {
    pub session_id: String,
    pub messages: Vec<MemoryMessage>,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemoryMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

// ── Billing API ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct UsageStats {
    pub user_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_requests: usize,
    pub total_tokens: usize,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cost_estimate: f64,
    pub quota_limit: usize,
    pub quota_used: usize,
    pub quota_remaining: usize,
}

#[derive(Debug, Serialize)]
pub struct RateLimitInfo {
    pub limit: usize,
    pub remaining: usize,
    pub reset_at: DateTime<Utc>,
    pub is_limited: bool,
}

impl RateLimitInfo {
    pub fn from_result(result: &crate::api::rate_limiter::RateLimitResult, reset_in_secs: u64) -> Self {
        Self {
            limit: result.limit,
            remaining: result.remaining,
            reset_at: Utc::now() + chrono::Duration::seconds(reset_in_secs as i64),
            is_limited: !result.allowed,
        }
    }
}

// ── API Response Wrapper ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitInfo>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            rate_limit: None,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
                details: None,
            }),
            rate_limit: None,
        }
    }

    pub fn with_rate_limit(self, rate_limit: RateLimitInfo) -> Self {
        Self {
            rate_limit: Some(rate_limit),
            ..self
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// ── Health API ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}
