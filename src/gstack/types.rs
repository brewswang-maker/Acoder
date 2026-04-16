//! gstack 执行状态类型（共享给 engine 和 exec_log）
//!
//! 避免 engine ↔ exec_log 循环依赖。

use serde::{Deserialize, Serialize};

/// 单步执行结果
#[derive(Debug, Clone)]
pub struct StepResult {
    pub step: u8,
    pub name: String,
    pub outcome: StepOutcome,
    pub tool_calls: usize,
    pub reasoning_tokens: u32,
    pub error: Option<String>,
}

/// 单步执行结果
#[derive(Debug, Clone)]
pub enum StepOutcome {
    Done,
    DoneWithConcerns(Vec<String>),
    Blocked(String),
    NeedsContext(Vec<String>),
    UserConfirm(Vec<crate::template::Choice>),
    Escalate(String),
}

/// 执行状态（跨模块共享）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecStatus {
    Success,
    DoneWithConcerns,
    Blocked,
    NeedsContext,
    Escalated,
    Skipped,
}

impl std::fmt::Display for ExecStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecStatus::Success => write!(f, "success"),
            ExecStatus::DoneWithConcerns => write!(f, "done_with_concerns"),
            ExecStatus::Blocked => write!(f, "blocked"),
            ExecStatus::NeedsContext => write!(f, "needs_context"),
            ExecStatus::Escalated => write!(f, "escalated"),
            ExecStatus::Skipped => write!(f, "skipped"),
        }
    }
}

impl From<&StepOutcome> for ExecStatus {
    fn from(o: &StepOutcome) -> Self {
        match o {
            StepOutcome::Done => ExecStatus::Success,
            StepOutcome::DoneWithConcerns(_) => ExecStatus::DoneWithConcerns,
            StepOutcome::Blocked(_) => ExecStatus::Blocked,
            StepOutcome::NeedsContext(_) => ExecStatus::NeedsContext,
            StepOutcome::UserConfirm(_) => ExecStatus::DoneWithConcerns,
            StepOutcome::Escalate(_) => ExecStatus::Escalated,
        }
    }
}
