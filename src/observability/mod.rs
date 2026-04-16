//! 观测域 — 指标收集与监控
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Metrics {
    tasks_total: AtomicU64,
    tasks_success: AtomicU64,
    tasks_failed: AtomicU64,
    tokens_total: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            tasks_total: AtomicU64::new(0),
            tasks_success: AtomicU64::new(0),
            tasks_failed: AtomicU64::new(0),
            tokens_total: AtomicU64::new(0),
        }
    }

    pub fn record_task_start(&self, _task: &str) {
        self.tasks_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_task_end(&self, result: &crate::execution::engine::ExecutionResult) {
        match result.status {
            crate::execution::engine::ExecutionStatus::Success => {
                self.tasks_success.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.tasks_failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.tokens_total.fetch_add(result.tokens_used as u64, Ordering::Relaxed);
    }

    pub fn summary(&self) -> MetricsSummary {
        let total = self.tasks_total.load(Ordering::Relaxed);
        let success = self.tasks_success.load(Ordering::Relaxed);
        MetricsSummary {
            tasks_total: total,
            tasks_success: success,
            tasks_failed: self.tasks_failed.load(Ordering::Relaxed),
            success_rate: if total > 0 { success as f64 / total as f64 } else { 0.0 },
            tokens_total: self.tokens_total.load(Ordering::Relaxed),
        }
    }
}

impl Default for Metrics { fn default() -> Self { Self::new() } }

#[derive(Debug)]
pub struct MetricsSummary {
    pub tasks_total: u64,
    pub tasks_success: u64,
    pub tasks_failed: u64,
    pub success_rate: f64,
    pub tokens_total: u64,
}
