//! # Intelligence — 自进化引擎
//!
//! 三层自适应系统的核心实现：
//!
//! ## L1 — 模型与重试调优
//! Multi-armed bandit（Thompson Sampling）根据历史成功率、延迟、成本
//! 为每个任务类型选择最佳模型 + 重试策略。
//!
//! ## L2 — 长任务规划
//! 接收长任务自动决定：并行度、子任务拆分、检查点间隔。
//!
//! ## L3 — Prompt 与记忆塑形
//! 每次请求后根据 OutcomeSignal 优化系统提示词和上下文长度。

pub mod outcome;
pub mod policy;
pub mod metrics;

pub use outcome::{OutcomeSignal, OutcomeRecorder, TaskType, ComplexityLevel};
pub use policy::{AdaptivePolicyEngine, PolicyConfig, ModelChoice};
pub use metrics::{
    UsageTracker, CostSummary,
    PolicyVersionManager, RolloutDecision, RolloutStatus,
    AuditEntry, AuditEvent, PolicyMetrics,
};
