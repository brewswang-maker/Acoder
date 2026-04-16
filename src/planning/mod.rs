//! 规划层（Planning）
//!
//! 决策中心：将复杂目标拆解为可执行的动作序列
//!
//! 核心组件：
//! - ReAct 循环
//! - 多 Agent 协作规划
//! - 动态编排选择
//! - CoT 推理模式
//! - 失败回路

pub mod react;
pub mod planner;
pub mod multi_agent;
pub mod dynamic_selector;
pub mod cot;
pub mod failure_loop;

use serde::{Deserialize, Serialize};

pub use react::ReActRunner;
pub use planner::{Planner, Plan, PlanStep, StepStatus, L2PlanDecision, RiskLevel, SplitHint};
pub use multi_agent::{MultiAgentPlanner, ExplorerRole};
pub use dynamic_selector::DynamicOrchestrator;
pub use cot::{CoTSelector, CoTMode};
pub use failure_loop::{FailureLoop, RepairStrategy};

/// 任务复杂度分层
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum TaskComplexity {
    /// 一句话能说清楚，改动不超过 1 个文件
    Simple,
    /// 需要理解多个模块，有隐性依赖
    Moderate,
    /// 涉及架构决策，影响多个子系统，需要并行协调
    Complex,
    /// 需要深入研究、实验、迭代才能找到正确方向
    Exploratory,
}

impl TaskComplexity {
    /// 根据任务描述推断复杂度
    pub fn infer_from(task: &str, file_count: usize) -> Self {
        let task_lower = task.to_lowercase();

        // 关键字推断
        let is_architecture = task_lower.contains("架构")
            || task_lower.contains("设计")
            || task_lower.contains("重构")
            || task_lower.contains("迁移");

        let is_exploratory = task_lower.contains("研究")
            || task_lower.contains("探索")
            || task_lower.contains("调研")
            || task_lower.contains("评估");

        let is_complex = file_count > 5
            || task_lower.contains("多个")
            || task_lower.contains("并行")
            || task_lower.contains("联调");

        if is_exploratory {
            TaskComplexity::Exploratory
        } else if is_architecture || is_complex {
            TaskComplexity::Complex
        } else if file_count > 1 || task_lower.contains("多个文件") {
            TaskComplexity::Moderate
        } else {
            TaskComplexity::Simple
        }
    }

    /// 获取建议的工作流类型
    pub fn suggested_workflow(&self) -> &'static str {
        match self {
            TaskComplexity::Simple => "react",
            TaskComplexity::Moderate => "plan_execute",
            TaskComplexity::Complex => "multi_agent",
            TaskComplexity::Exploratory => "exploratory",
        }
    }
}
