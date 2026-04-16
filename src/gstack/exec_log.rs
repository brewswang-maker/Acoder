//! gstack 执行日志 & 自进化信号
//!
//! 记录每个 step 的执行状态，供 acode 的 skill/evolution.rs 消费，
//! 构成 "OutcomeSignal → 归因 → 改进" 的反馈闭环。

use serde::{Deserialize, Serialize};

pub use crate::gstack::types::ExecStatus;

/// 单个 step 的执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub step: u8,
    pub name: String,
    pub status: ExecStatus,
    pub tool_calls: usize,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

/// 执行日志
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecLog {
    /// skill 元信息
    pub skill: String,
    pub version: String,
    pub branch: String,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    /// step 记录（按顺序）
    pub steps: Vec<StepRecord>,
    /// 最终状态
    pub final_status: Option<ExecStatus>,
    /// 汇总
    pub summary: Option<ExecSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecSummary {
    pub total_steps: usize,
    pub completed_steps: usize,
    pub total_tool_calls: usize,
    pub total_elapsed_ms: u64,
    pub concerns: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub needs_context: Vec<String>,
}

impl ExecLog {
    /// 记录单个 step
    pub fn record_step(&mut self, step: u8, name: String, status: ExecStatus) {
        self.steps.push(StepRecord {
            step,
            name,
            status,
            tool_calls: 0,
            error: None,
            elapsed_ms: 0,
        });
    }

    /// 填充 step 详细信息
    pub fn fill_step(&mut self, step: u8, tool_calls: usize, elapsed_ms: u64, error: Option<String>) {
        if let Some(r) = self.steps.iter_mut().find(|r| r.step == step) {
            r.tool_calls = tool_calls;
            r.elapsed_ms = elapsed_ms;
            r.error = error;
        }
    }

    /// 完成日志，生成 summary
    pub fn finalize(&mut self) {
        self.completed_at = Some(now_ts());
        let completed = self
            .steps
            .iter()
            .filter(|s| s.status == ExecStatus::Success || s.status == ExecStatus::DoneWithConcerns)
            .count();
        let total_calls: usize = self.steps.iter().map(|s| s.tool_calls).sum();
        let total_ms: u64 = self.steps.iter().map(|s| s.elapsed_ms).sum();
        let concerns: Vec<String> = self
            .steps
            .iter()
            .filter(|s| s.status == ExecStatus::DoneWithConcerns)
            .map(|s| s.name.clone())
            .collect();
        let blocked: Vec<String> = self
            .steps
            .iter()
            .filter(|s| s.status == ExecStatus::Blocked)
            .map(|s| s.name.clone())
            .collect();
        let needs_ctx: Vec<String> = self
            .steps
            .iter()
            .filter(|s| s.status == ExecStatus::NeedsContext)
            .map(|s| s.name.clone())
            .collect();

        self.final_status = Some(if !blocked.is_empty() {
            ExecStatus::Blocked
        } else if !needs_ctx.is_empty() {
            ExecStatus::NeedsContext
        } else if self.steps.iter().any(|s| s.status == ExecStatus::Escalated) {
            ExecStatus::Escalated
        } else if !concerns.is_empty() {
            ExecStatus::DoneWithConcerns
        } else {
            ExecStatus::Success
        });

        self.summary = Some(ExecSummary {
            total_steps: self.steps.len(),
            completed_steps: completed,
            total_tool_calls: total_calls,
            total_elapsed_ms: total_ms,
            concerns,
            blocked_reasons: blocked,
            needs_context: needs_ctx,
        });
    }

    /// 转换为 acode OutcomeSignal（供 skill/evolution.rs 消费）
    pub fn to_outcome_signal(&self) -> OutcomeSignal {
        let success_rate = if self.steps.is_empty() {
            0.0
        } else {
            self.steps
                .iter()
                .filter(|s| s.status == ExecStatus::Success || s.status == ExecStatus::DoneWithConcerns)
                .count() as f64
                / self.steps.len() as f64
        };
        OutcomeSignal {
            skill: self.skill.clone(),
            version: self.version.clone(),
            branch: self.branch.clone(),
            status: self.final_status.unwrap_or(ExecStatus::Blocked),
            success_rate,
            tool_calls: self.summary.as_ref().map(|s| s.total_tool_calls).unwrap_or(0),
            elapsed_ms: self.summary.as_ref().map(|s| s.total_elapsed_ms).unwrap_or(0),
            blocked_reasons: self.summary.as_ref().map(|s| s.blocked_reasons.clone()).unwrap_or_default(),
            concerns: self.summary.as_ref().map(|s| s.concerns.clone()).unwrap_or_default(),
        }
    }

    /// 导出 JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// acode skill/evolution.rs 的 OutcomeSignal（简化版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeSignal {
    pub skill: String,
    pub version: String,
    pub branch: String,
    pub status: ExecStatus,
    /// 成功率 (0.0 - 1.0)
    pub success_rate: f64,
    pub tool_calls: usize,
    pub elapsed_ms: u64,
    pub blocked_reasons: Vec<String>,
    pub concerns: Vec<String>,
}

/// 从 engine StepResult 批量构建 ExecLog
impl From<Vec<crate::gstack::engine::StepResult>> for ExecLog {
    fn from(step_results: Vec<crate::gstack::engine::StepResult>) -> Self {
        let mut log = ExecLog::default();
        for r in &step_results {
            let status = ExecStatus::from(&r.outcome);
            log.record_step(r.step, r.name.clone(), status);
        }
        log.finalize();
        log
    }
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
