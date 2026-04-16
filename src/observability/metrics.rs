//! 指标收集系统
//!
//! 核心指标收集与查询，用于观测 ACoder 运行状态。
//! 内部使用 parking_lot::RwLock 保证线程安全。
//!
//! 收集的指标：
//! - 任务执行指标（TaskMetric）
//! - LLM 调用指标（LlmMetric）
//! - 工具调用指标（ToolMetric）

use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

// ── 原始指标结构 ──────────────────────────────────────────────────────────

/// 任务执行指标
#[derive(Debug, Clone)]
pub struct TaskMetric {
    /// 任务描述
    pub task: String,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
    /// 消耗 Token 数
    pub tokens: u64,
    /// 是否成功
    pub success: bool,
    /// 记录时间
    pub recorded_at: Instant,
}

/// LLM 调用指标
#[derive(Debug, Clone)]
pub struct LlmMetric {
    /// 模型名称
    pub model: String,
    /// 输入 Token 数
    pub tokens_in: u64,
    /// 输出 Token 数
    pub tokens_out: u64,
    /// 调用延迟（毫秒）
    pub latency_ms: u64,
    /// 是否成功
    pub success: bool,
    /// 记录时间
    pub recorded_at: Instant,
}

/// 工具调用指标
#[derive(Debug, Clone)]
pub struct ToolMetric {
    /// 工具名称
    pub tool: String,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
    /// 是否成功
    pub success: bool,
    /// 记录时间
    pub recorded_at: Instant,
}

// ── 指标汇总 ──────────────────────────────────────────────────────────────

/// 指标汇总快照
#[derive(Debug, Clone, Default)]
pub struct MetricsSummary {
    /// 总任务数
    pub total_tasks: u64,
    /// 成功任务数
    pub successful_tasks: u64,
    /// 任务成功率（0.0 ~ 1.0）
    pub task_success_rate: f64,
    /// 平均任务 Token 消耗
    pub avg_tokens_per_task: f64,
    /// 总 Token 消耗
    pub total_tokens: u64,
    /// LLM 调用次数
    pub llm_call_count: u64,
    /// LLM 成功调用次数
    pub llm_success_count: u64,
    /// LLM 平均延迟（毫秒）
    pub avg_llm_latency_ms: f64,
    /// 工具调用次数
    pub tool_call_count: u64,
    /// 工具成功调用次数
    pub tool_success_count: u64,
    /// 工具平均耗时（毫秒）
    pub avg_tool_duration_ms: f64,
}

// ── 内部存储 ──────────────────────────────────────────────────────────────

/// 指标存储（内部结构，受 RwLock 保护）
#[derive(Debug, Default)]
struct MetricsStore {
    tasks: VecDeque<TaskMetric>,
    llm_calls: VecDeque<LlmMetric>,
    tool_calls: VecDeque<ToolMetric>,
}

impl MetricsStore {
    fn new() -> Self {
        Self {
            tasks: VecDeque::with_capacity(10000),
            llm_calls: VecDeque::with_capacity(10000),
            tool_calls: VecDeque::with_capacity(10000),
        }
    }

    /// 最大保留条目数
    const MAX_ENTRIES: usize = 10000;
}

// ── 指标收集器 ────────────────────────────────────────────────────────────

/// 指标收集器
///
/// 线程安全的指标收集与查询入口。
pub struct MetricsCollector {
    store: Arc<RwLock<MetricsStore>>,
}

// MetricsCollector 是 Send + Sync（RwLock<T> 当 T: Send 时满足）
static_assertions::assert_impl_all!(MetricsCollector: Send, Sync);

impl MetricsCollector {
    /// 创建新的指标收集器
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(MetricsStore::new())),
        }
    }

    /// 记录任务指标
    pub fn record_task(
        &self,
        task: impl Into<String>,
        duration_ms: u64,
        tokens: u64,
        success: bool,
    ) {
        let metric = TaskMetric {
            task: task.into(),
            duration_ms,
            tokens,
            success,
            recorded_at: Instant::now(),
        };
        let mut store = self.store.write();
        if store.tasks.len() >= MetricsStore::MAX_ENTRIES {
            store.tasks.pop_front();
        }
        store.tasks.push_back(metric);
    }

    /// 记录 LLM 调用指标
    pub fn record_llm_call(
        &self,
        model: impl Into<String>,
        tokens_in: u64,
        tokens_out: u64,
        latency_ms: u64,
        success: bool,
    ) {
        let metric = LlmMetric {
            model: model.into(),
            tokens_in,
            tokens_out,
            latency_ms,
            success,
            recorded_at: Instant::now(),
        };
        let mut store = self.store.write();
        if store.llm_calls.len() >= MetricsStore::MAX_ENTRIES {
            store.llm_calls.pop_front();
        }
        store.llm_calls.push_back(metric);
    }

    /// 记录工具调用指标
    pub fn record_tool_call(
        &self,
        tool: impl Into<String>,
        duration_ms: u64,
        success: bool,
    ) {
        let metric = ToolMetric {
            tool: tool.into(),
            duration_ms,
            success,
            recorded_at: Instant::now(),
        };
        let mut store = self.store.write();
        if store.tool_calls.len() >= MetricsStore::MAX_ENTRIES {
            store.tool_calls.pop_front();
        }
        store.tool_calls.push_back(metric);
    }

    /// 获取指标汇总快照
    pub fn summary(&self) -> MetricsSummary {
        let store = self.store.read();

        // 任务指标汇总
        let total_tasks = store.tasks.len() as u64;
        let successful_tasks = store.tasks.iter().filter(|t| t.success).count() as u64;
        let total_tokens: u64 = store.tasks.iter().map(|t| t.tokens).sum();
        let task_success_rate = if total_tasks > 0 {
            successful_tasks as f64 / total_tasks as f64
        } else {
            0.0
        };
        let avg_tokens_per_task = if total_tasks > 0 {
            total_tokens as f64 / total_tasks as f64
        } else {
            0.0
        };

        // LLM 调用汇总
        let llm_call_count = store.llm_calls.len() as u64;
        let llm_success_count = store.llm_calls.iter().filter(|l| l.success).count() as u64;
        let avg_llm_latency_ms = if llm_call_count > 0 {
            store.llm_calls.iter().map(|l| l.latency_ms).sum::<u64>() as f64
                / llm_call_count as f64
        } else {
            0.0
        };

        // 工具调用汇总
        let tool_call_count = store.tool_calls.len() as u64;
        let tool_success_count = store.tool_calls.iter().filter(|t| t.success).count() as u64;
        let avg_tool_duration_ms = if tool_call_count > 0 {
            store.tool_calls.iter().map(|t| t.duration_ms).sum::<u64>() as f64
                / tool_call_count as f64
        } else {
            0.0
        };

        MetricsSummary {
            total_tasks,
            successful_tasks,
            task_success_rate,
            avg_tokens_per_task,
            total_tokens,
            llm_call_count,
            llm_success_count,
            avg_llm_latency_ms,
            tool_call_count,
            tool_success_count,
            avg_tool_duration_ms,
        }
    }

    /// 获取任务指标快照（最近 N 条）
    pub fn recent_tasks(&self, n: usize) -> Vec<TaskMetric> {
        let store = self.store.read();
        store.tasks.iter().rev().take(n).cloned().collect()
    }

    /// 获取 LLM 调用指标快照（最近 N 条）
    pub fn recent_llm_calls(&self, n: usize) -> Vec<LlmMetric> {
        let store = self.store.read();
        store.llm_calls.iter().rev().take(n).cloned().collect()
    }

    /// 获取工具调用指标快照（最近 N 条）
    pub fn recent_tool_calls(&self, n: usize) -> Vec<ToolMetric> {
        let store = self.store.read();
        store.tool_calls.iter().rev().take(n).cloned().collect()
    }

    /// 重置所有指标
    pub fn reset(&self) {
        let mut store = self.store.write();
        store.tasks.clear();
        store.llm_calls.clear();
        store.tool_calls.clear();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== ACoder Metrics Summary ===")?;
        writeln!(f, "Tasks:      {} total, {} success ({:.1}%)",
            self.total_tasks, self.successful_tasks,
            self.task_success_rate * 100.0)?;
        writeln!(f, "Tokens:     {} total, {:.0} avg/task",
            self.total_tokens, self.avg_tokens_per_task)?;
        writeln!(f, "LLM Calls:  {} total, {} success, {:.0}ms avg",
            self.llm_call_count, self.llm_success_count,
            self.avg_llm_latency_ms)?;
        writeln!(f, "Tool Calls: {} total, {} success, {:.0}ms avg",
            self.tool_call_count, self.tool_success_count,
            self.avg_tool_duration_ms)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_summary() {
        let collector = MetricsCollector::new();

        // 记录任务
        collector.record_task("写排序函数", 1200, 500, true);
        collector.record_task("写测试", 800, 300, true);
        collector.record_task("修复 bug", 2000, 800, false);

        let summary = collector.summary();
        assert_eq!(summary.total_tasks, 3);
        assert_eq!(summary.successful_tasks, 2);
        assert!((summary.task_success_rate - 2.0 / 3.0).abs() < 0.01);
        assert_eq!(summary.total_tokens, 1600);
    }

    #[test]
    fn test_llm_metrics() {
        let collector = MetricsCollector::new();

        collector.record_llm_call("gpt-4o", 100, 200, 1500, true);
        collector.record_llm_call("gpt-4o", 150, 300, 2000, true);
        collector.record_llm_call("claude-3", 200, 100, 3000, false);

        let summary = collector.summary();
        assert_eq!(summary.llm_call_count, 3);
        assert_eq!(summary.llm_success_count, 2);
        assert!((summary.avg_llm_latency_ms - 2166.67).abs() < 1.0);
    }

    #[test]
    fn test_tool_metrics() {
        let collector = MetricsCollector::new();

        collector.record_tool_call("file_read", 50, true);
        collector.record_tool_call("bash", 200, true);
        collector.record_tool_call("file_write", 100, false);

        let summary = collector.summary();
        assert_eq!(summary.tool_call_count, 3);
        assert_eq!(summary.tool_success_count, 2);
        assert!((summary.avg_tool_duration_ms - 116.67).abs() < 1.0);
    }

    #[test]
    fn test_recent_tasks() {
        let collector = MetricsCollector::new();

        for i in 0..10 {
            collector.record_task(format!("task-{}", i), 100, 50, i % 2 == 0);
        }

        let recent = collector.recent_tasks(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].task, "task-9");
        assert_eq!(recent[1].task, "task-8");
        assert_eq!(recent[2].task, "task-7");
    }

    #[test]
    fn test_reset() {
        let collector = MetricsCollector::new();
        collector.record_task("test", 100, 50, true);
        collector.record_llm_call("gpt-4o", 100, 100, 500, true);
        collector.record_tool_call("bash", 50, true);

        collector.reset();

        let summary = collector.summary();
        assert_eq!(summary.total_tasks, 0);
        assert_eq!(summary.llm_call_count, 0);
        assert_eq!(summary.tool_call_count, 0);
    }

    #[test]
    fn test_display_summary() {
        let collector = MetricsCollector::new();
        collector.record_task("test", 100, 50, true);
        let summary = collector.summary();
        let text = format!("{}", summary);
        assert!(text.contains("ACoder Metrics Summary"));
        assert!(text.contains("Tasks:"));
    }

    #[test]
    fn test_empty_summary() {
        let collector = MetricsCollector::new();
        let summary = collector.summary();
        assert_eq!(summary.total_tasks, 0);
        assert_eq!(summary.task_success_rate, 0.0);
        assert_eq!(summary.avg_llm_latency_ms, 0.0);
    }
}
