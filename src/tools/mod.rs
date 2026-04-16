//! 工具模块
//!
//! 运维与监控工具：
//! - 健康监控：系统资源 + LLM 可用性
//! - 性能指标：Token 消耗 / 延迟 / 成功率

pub mod health_monitor;
pub mod builtin;

pub use health_monitor::HealthMonitor;
pub use builtin::BuiltinToolExecutor;

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

impl ToolOutput {
    pub fn success(output: &str) -> Self {
        Self {
            stdout: output.to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 0,
        }
    }

    pub fn error(msg: &str) -> Self {
        Self {
            stdout: String::new(),
            stderr: msg.to_string(),
            exit_code: 1,
            duration_ms: 0,
        }
    }

    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

/// 工具指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetrics {
    pub name: String,
    pub call_count: usize,
    pub success_count: usize,
    pub avg_duration_ms: f64,
    pub last_called_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 工具指标追踪器
pub struct ToolMetricsTracker {
    metrics: std::collections::HashMap<String, ToolMetricsInner>,
}

#[derive(Default)]
struct ToolMetricsInner {
    call_count: usize,
    success_count: usize,
    total_duration_ms: u64,
    recent_durations: VecDeque<u64>,
}

impl ToolMetricsTracker {
    pub fn new() -> Self {
        Self {
            metrics: std::collections::HashMap::new(),
        }
    }

    /// 记录工具调用
    pub fn record(&mut self, tool: &str, duration_ms: u64, success: bool) {
        let entry = self.metrics.entry(tool.to_string()).or_default();
        entry.call_count += 1;
        if success { entry.success_count += 1; }
        entry.total_duration_ms += duration_ms;
        entry.recent_durations.push_back(duration_ms);
        if entry.recent_durations.len() > 100 {
            entry.recent_durations.pop_front();
        }
    }

    /// 获取工具指标
    pub fn get_metrics(&self, tool: &str) -> Option<ToolMetrics> {
        self.metrics.get(tool).map(|inner| ToolMetrics {
            name: tool.to_string(),
            call_count: inner.call_count,
            success_count: inner.success_count,
            avg_duration_ms: if inner.call_count > 0 {
                inner.total_duration_ms as f64 / inner.call_count as f64
            } else { 0.0 },
            last_called_at: None,
        })
    }

    /// 获取所有工具指标
    pub fn all_metrics(&self) -> Vec<ToolMetrics> {
        self.metrics.keys().filter_map(|k| self.get_metrics(k)).collect()
    }
}

impl Default for ToolMetricsTracker {
    fn default() -> Self { Self::new() }
}
