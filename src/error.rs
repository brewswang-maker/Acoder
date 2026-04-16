//! Acode 错误类型定义
//!
//! 使用 thiserror + anyhow 混合模式：
//! - thiserror: 用于定义已知错误变体（类型安全、匹配友好）
//! - anyhow: 用于传播未知错误（? 操作符、上下文丰富）

use thiserror::Error;
use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error, Clone)]
pub enum Error {
    // ── 核心错误 ──────────────────────────────────────────────

    #[error("任务执行失败: {0}")]
    TaskFailed(String),

    #[error("任务超时: {0}")]
    TaskTimeout(String),

    #[error("任务被取消: {task_id}")]
    TaskCancelled { task_id: String },

    #[error("任务未找到: {task_id}")]
    TaskNotFound { task_id: String },

    // ── LLM 错误 ─────────────────────────────────────────────

    #[error("LLM 调用失败: {reason}")]
    LlmFailed { reason: String },

    #[error("LLM 请求被限流: {retry_after}s 后重试")]
    LlmRateLimited { retry_after: u64 },

    #[error("LLM 模型不可用: {model}")]
    LlmModelUnavailable { model: String },

    #[error("LLM Token 超限: 请求 {request_tokens}, 上限 {max_tokens}")]
    LlmTokenLimit { request_tokens: usize, max_tokens: usize },

    #[error("LLM API Key 未配置或无效")]
    LlmAuthFailed,

    // ── 上下文错误 ────────────────────────────────────────────

    #[error("上下文加载失败: {0}")]
    ContextLoadFailed(String),

    #[error("上下文超限: {size} tokens, 限制 {limit} tokens")]
    ContextOverflow { size: usize, limit: usize },

    #[error("项目路径不存在: {path}")]
    ProjectNotFound { path: PathBuf },

    #[error("外部工具执行失败 [{tool}]: {reason}")]
    ExternalToolError { tool: String, reason: String },

    #[error("IO 错误: {0}")]
    IoError(String),

    #[error("文件不存在: {path}")]
    FileNotFound { path: PathBuf },

    // ── 执行错误 ─────────────────────────────────────────────

    #[error("代码执行失败 [{lang}]: {reason}")]
    ExecutionFailed { lang: String, reason: String },

    #[error("沙箱执行超时: {timeout}s")]
    SandboxTimeout { timeout: u64 },

    #[error("沙箱资源超限: {resource}")]
    SandboxResourceLimit { resource: String },

    #[error("沙箱安全拦截: {operation} — {reason}")]
    SandboxSecurityBlocked { operation: String, reason: String },

    // ── Agent 错误 ───────────────────────────────────────────

    #[error("Agent 未找到: {agent_id}")]
    AgentNotFound { agent_id: String },

    #[error("Agent 执行失败 [{agent_id}]: {reason}")]
    AgentExecutionFailed { agent_id: String, reason: String },

    #[error("Agent 协作超时: {task_id}")]
    AgentCollaborationTimeout { task_id: String },

    #[error("Agent 间通信失败: {from} → {to}")]
    AgentCommFailed { from: String, to: String, reason: String },

    // ── Skill 错误 ───────────────────────────────────────────

    #[error("Skill 未找到: {skill_id}")]
    SkillNotFound { skill_id: String },

    #[error("Skill 执行失败 [{skill_id}]: {reason}")]
    SkillFailed { skill_id: String, reason: String },

    #[error("Skill 验证失败: {skill_id} — {reason}")]
    SkillValidationFailed { skill_id: String, reason: String },

    #[error("Skill 进化失败: {skill_id}")]
    SkillEvolutionFailed { skill_id: String },

    #[error("Skill 反模式违规: {patterns:?}")]
    SkillAntiPatternViolation { patterns: Vec<String> },

    // ── 规划错误 ─────────────────────────────────────────────

    #[error("规划失败: {0}")]
    PlanningFailed(String),

    #[error("计划被拒绝")]
    PlanRejected,

    #[error("计划执行中断: 在步骤 {step}/{total}")]
    PlanInterrupted { step: usize, total: usize },

    // ── 记忆错误 ─────────────────────────────────────────────

    #[error("记忆存储失败: {0}")]
    MemoryStoreFailed(String),

    #[error("记忆检索失败: {0}")]
    MemoryRetrievalFailed(String),

    #[error("会话未找到: {session_id}")]
    SessionNotFound { session_id: String },

    // ── 配置错误 ─────────────────────────────────────────────

    #[error("配置未找到: {key}")]
    ConfigMissing { key: String },

    #[error("配置无效: {key} — {reason}")]
    ConfigInvalid { key: String, reason: String },

    #[error("环境变量未设置: {var}")]
    EnvVarMissing { var: String },

    // ── 网关错误 ─────────────────────────────────────────────

    #[error("网关请求失败: {0}")]
    GatewayRequestFailed(String),

    #[error("网关路由失败: {reason}")]
    GatewayRouteFailed { reason: String },

    #[error("限流触发: {user_id} — 超出 {limit} req/min")]
    RateLimited { user_id: String, limit: u32 },

    #[error("认证失败: {reason}")]
    AuthFailed { reason: String },

    // ── 安全错误 ─────────────────────────────────────────────

    #[error("权限不足: {action}")]
    PermissionDenied { action: String },

    #[error("安全策略拦截: {policy} — {reason}")]
    SecurityPolicyBlocked { policy: String, reason: String },

    #[error("审批超时: {action}")]
    ApprovalTimeout { action: String },

    // ── 工具错误 ─────────────────────────────────────────────

    #[error("工具未注册: {tool_name}")]
    ToolNotFound { tool_name: String },

    #[error("工具调用失败 [{tool_name}]: {reason}")]
    ToolCallFailed { tool_name: String, reason: String },

    #[error("工具健康度低: {tool_name} ({health:.1}%成功率)")]
    ToolHealthLow { tool_name: String, health: f64 },

    // ── MCP 错误 ─────────────────────────────────────────────

    #[error("MCP 服务器连接失败: {server}")]
    McpServerConnectFailed { server: String },

    #[error("MCP 协议错误: {0}")]
    McpProtocolError(String),
}

// ── 便捷构造方法 ──────────────────────────────────────────────

impl Error {
    /// 带上下文链创建错误
    pub fn context(self, msg: impl ToString) -> anyhow::Error {
        anyhow::anyhow!("{}: {}", msg.to_string(), self)
    }

    /// 判断是否为可重试错误
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::LlmRateLimited { .. }
                | Error::TaskTimeout(_)
                | Error::LlmFailed { .. }
                | Error::McpServerConnectFailed { .. }
        )
    }

    /// 获取错误代码（用于日志和指标）
    pub fn code(&self) -> &'static str {
        match self {
            Error::TaskFailed(_) => "TASK_FAILED",
            Error::LlmFailed { .. } => "LLM_FAILED",
            Error::LlmRateLimited { .. } => "LLM_RATE_LIMITED",
            Error::LlmTokenLimit { .. } => "LLM_TOKEN_LIMIT",
            Error::ContextOverflow { .. } => "CONTEXT_OVERFLOW",
            Error::ExecutionFailed { .. } => "EXECUTION_FAILED",
            Error::SandboxTimeout { .. } => "SANDBOX_TIMEOUT",
            Error::SkillFailed { .. } => "SKILL_FAILED",
            Error::PlanningFailed(_) => "PLANNING_FAILED",
            Error::SecurityPolicyBlocked { .. } => "SECURITY_BLOCKED",
            _ => "INTERNAL_ERROR",
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::ContextLoadFailed(e.to_string())
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        Error::TaskTimeout("操作超时".into())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::LlmFailed { reason: "请求超时".into() }
        } else if e.is_connect() {
            Error::McpServerConnectFailed { server: e.url().map(|u| u.to_string()).unwrap_or_default() }
        } else {
            Error::LlmFailed { reason: e.to_string() }
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::ContextLoadFailed(format!("Database error: {}", e))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::ContextLoadFailed(format!("JSON error: {}", e))
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::IoError(e.to_string())
    }
}


