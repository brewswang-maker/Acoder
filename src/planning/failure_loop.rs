//! 失败回路 — Retry / Replan / Escalate / Abort
//!
//! 任务失败时的系统性处理策略：
//! - Retry: 重试同一方案（瞬态错误）
//! - Replan: 重新规划（方案本身有问题）
//! - Escalate: 上报人工处理（超出自愈范围）
//! - Abort: 终止任务（不可恢复错误）
//!
//! 参考 hermes-agent-rs 失败回路设计：
//! 每次失败归因分析后自动选择修复策略，避免无脑重试

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::error::Result;

/// 修复策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum RepairStrategy {
    /// 重试同一方案（网络超时、API 限流等瞬态错误）
    Retry,
    /// 重新规划方案（思路方向错误）
    Replan,
    /// 升级处理（切换到更强模型 / 请求人工介入）
    Escalate,
    /// 终止任务（不可恢复错误）
    Abort,
}

/// 失败记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    pub step_id: String,
    pub error_message: String,
    pub error_category: ErrorCategory,
    pub repair_strategy: RepairStrategy,
    pub retry_count: usize,
    pub timestamp: DateTime<Utc>,
}

/// 错误分类（决定修复策略的关键）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// 网络超时 / API 限流 → Retry
    Transient,
    /// LLM 输出格式错误 / 幻觉 → Replan
    LlmError,
    /// 工具不存在 / 参数错误 → Replan
    ToolError,
    /// 文件不存在 / 权限不足 → Escalate
    EnvironmentError,
    /// 安全策略拦截 / 人工审批拒绝 → Abort
    PolicyError,
    /// 未知错误 → Escalate
    Unknown,
}

impl ErrorCategory {
    /// 从错误消息推断错误分类
    pub fn infer_from(error: &str) -> Self {
        let lower = error.to_lowercase();

        // Transient: 网络和限流错误
        if lower.contains("timeout") || lower.contains("timed out")
            || lower.contains("rate limit") || lower.contains("429")
            || lower.contains("connection refused") || lower.contains("网络")
            || lower.contains("超时")
        {
            return Self::Transient;
        }

        // LLM Error: 输出格式和幻觉
        if lower.contains("json parse") || lower.contains("invalid json")
            || lower.contains("unexpected format") || lower.contains("hallucination")
            || lower.contains("格式错误") || lower.contains("幻觉")
        {
            return Self::LlmError;
        }

        // Tool Error: 工具和参数
        if lower.contains("tool not found") || lower.contains("unknown tool")
            || lower.contains("invalid argument") || lower.contains("工具不存在")
            || lower.contains("参数错误")
        {
            return Self::ToolError;
        }

        // Environment Error: 文件和权限
        if lower.contains("permission denied") || lower.contains("file not found")
            || lower.contains("enoent") || lower.contains("权限不足")
            || lower.contains("文件不存在")
        {
            return Self::EnvironmentError;
        }

        // Policy Error: 安全策略
        if lower.contains("blocked by policy") || lower.contains("approval rejected")
            || lower.contains("安全策略") || lower.contains("审批拒绝")
        {
            return Self::PolicyError;
        }

        Self::Unknown
    }

    /// 根据错误分类推荐修复策略
    pub fn recommended_strategy(&self) -> RepairStrategy {
        match self {
            Self::Transient => RepairStrategy::Retry,
            Self::LlmError | Self::ToolError => RepairStrategy::Replan,
            Self::EnvironmentError | Self::Unknown => RepairStrategy::Escalate,
            Self::PolicyError => RepairStrategy::Abort,
        }
    }
}

/// 失败回路处理器
pub struct FailureLoop {
    /// 最大重试次数
    max_retries: usize,
    /// 最大重新规划次数
    max_replans: usize,
    /// 历史失败记录
    failure_history: Vec<FailureRecord>,
}

impl FailureLoop {
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            max_replans: 2,
            failure_history: Vec::new(),
        }
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max: usize) -> Self {
        self.max_retries = max;
        self
    }

    /// 处理失败，返回修复策略
    pub fn handle_failure(
        &mut self,
        step_id: &str,
        error: &str,
        retry_count: usize,
        replan_count: usize,
    ) -> RepairStrategy {
        let category = ErrorCategory::infer_from(error);
        let mut strategy = category.recommended_strategy();

        // 检查重试上限
        if strategy == RepairStrategy::Retry && retry_count >= self.max_retries {
            tracing::warn!(
                "重试次数已达上限 ({}/{}), 升级为 Replan",
                retry_count, self.max_retries
            );
            strategy = RepairStrategy::Replan;
        }

        // 检查重新规划上限
        if strategy == RepairStrategy::Replan && replan_count >= self.max_replans {
            tracing::warn!(
                "重新规划次数已达上限 ({}/{}), 升级为 Escalate",
                replan_count, self.max_replans
            );
            strategy = RepairStrategy::Escalate;
        }

        // 记录失败
        self.failure_history.push(FailureRecord {
            step_id: step_id.to_string(),
            error_message: error.to_string(),
            error_category: category,
            repair_strategy: strategy,
            retry_count,
            timestamp: Utc::now(),
        });

        tracing::info!(
            "失败处理 | step: {} | category: {:?} | strategy: {:?}",
            step_id, category, strategy
        );

        strategy
    }

    /// 获取某个步骤的失败次数
    pub fn failure_count_for(&self, step_id: &str) -> usize {
        self.failure_history.iter()
            .filter(|f| f.step_id == step_id)
            .count()
    }

    /// 获取所有失败记录
    pub fn history(&self) -> &[FailureRecord] {
        &self.failure_history
    }

    /// 清空失败历史
    pub fn reset(&mut self) {
        self.failure_history.clear();
    }

    /// 生成失败报告
    pub fn generate_report(&self) -> FailureReport {
        let total = self.failure_history.len();
        let retries = self.failure_history.iter()
            .filter(|f| f.repair_strategy == RepairStrategy::Retry)
            .count();
        let replans = self.failure_history.iter()
            .filter(|f| f.repair_strategy == RepairStrategy::Replan)
            .count();
        let escalations = self.failure_history.iter()
            .filter(|f| f.repair_strategy == RepairStrategy::Escalate)
            .count();
        let aborts = self.failure_history.iter()
            .filter(|f| f.repair_strategy == RepairStrategy::Abort)
            .count();

        FailureReport {
            total_failures: total,
            retry_count: retries,
            replan_count: replans,
            escalation_count: escalations,
            abort_count: aborts,
            records: self.failure_history.clone(),
        }
    }
}

impl Default for FailureLoop {
    fn default() -> Self {
        Self::new()
    }
}

/// 失败报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureReport {
    pub total_failures: usize,
    pub retry_count: usize,
    pub replan_count: usize,
    pub escalation_count: usize,
    pub abort_count: usize,
    pub records: Vec<FailureRecord>,
}
