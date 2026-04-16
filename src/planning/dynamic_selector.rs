//! 动态编排选择器 — 根据任务特征选择最优工作流
//!
//! 三种工作流：
//! - ReAct: 简单任务，单 Agent 迭代
//! - Plan-Execute: 中等任务，先规划后执行
//! - Multi-Agent: 复杂任务，多专家并行
//!
//! 选择依据：任务复杂度、文件影响范围、风险等级、时间约束

use serde::{Deserialize, Serialize};

use crate::planning::{TaskComplexity, planner::RiskLevel};
use super::react::ReActRunner;
use super::failure_loop::RepairStrategy;

/// 工作流类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    /// ReAct 循环（简单任务）
    ReAct,
    /// 规划-执行（中等任务）
    PlanExecute,
    /// 多 Agent 协作（复杂任务）
    MultiAgent,
    /// 探索式（研究方向）
    Exploratory,
}

/// 编排决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationDecision {
    /// 选择的工作流
    pub workflow: WorkflowType,
    /// 选择理由
    pub reason: String,
    /// 推荐并行度
    pub parallelism: usize,
    /// 是否需要人工审批
    pub requires_approval: bool,
    /// 风险等级
    pub risk: RiskLevel,
    /// 估算步骤数
    pub estimated_steps: usize,
}

/// 动态编排器
pub struct DynamicOrchestrator {
    /// 是否允许自动升级工作流（ReAct → PlanExecute → MultiAgent）
    auto_escalate: bool,
}

impl DynamicOrchestrator {
    pub fn new() -> Self {
        Self { auto_escalate: true }
    }

    /// 根据任务特征选择工作流
    pub fn decide(
        &self,
        task: &str,
        complexity: TaskComplexity,
        affected_files: usize,
        risk: RiskLevel,
    ) -> OrchestrationDecision {
        let (workflow, reason, parallelism, requires_approval, estimated_steps) =
            self.evaluate(task, complexity, affected_files, risk);

        OrchestrationDecision {
            workflow,
            reason,
            parallelism,
            requires_approval,
            risk,
            estimated_steps,
        }
    }

    fn evaluate(
        &self,
        task: &str,
        complexity: TaskComplexity,
        affected_files: usize,
        risk: RiskLevel,
    ) -> (WorkflowType, String, usize, bool, usize) {
        // 高风险 → 强制人工审批
        let requires_approval = matches!(risk, RiskLevel::High | RiskLevel::Critical);

        match complexity {
            TaskComplexity::Simple => (
                WorkflowType::ReAct,
                "简单任务，使用 ReAct 单步迭代即可完成".into(),
                1,
                requires_approval,
                3,
            ),
            TaskComplexity::Moderate => {
                // 中等复杂度，但影响文件多 → 升级为 MultiAgent
                if affected_files > 5 {
                    (
                        WorkflowType::MultiAgent,
                        format!("中等任务但影响 {} 个文件，升级为多 Agent 并行", affected_files),
                        3,
                        requires_approval,
                        8,
                    )
                } else {
                    (
                        WorkflowType::PlanExecute,
                        "中等任务，先规划后执行更可控".into(),
                        2,
                        requires_approval,
                        5,
                    )
                }
            }
            TaskComplexity::Complex => (
                WorkflowType::MultiAgent,
                "复杂任务，需要多专家协作".into(),
                4,
                true, // 复杂任务默认需要审批
                12,
            ),
            TaskComplexity::Exploratory => (
                WorkflowType::Exploratory,
                "研究方向，需要迭代探索".into(),
                2,
                false,
                20,
            ),
        }
    }

    /// 任务执行中动态升级工作流
    ///
    /// 当 ReAct 循环连续失败时，自动升级为 PlanExecute 或 MultiAgent
    pub fn should_escalate(
        &self,
        current: WorkflowType,
        failure_count: usize,
        repair_strategy: RepairStrategy,
    ) -> Option<WorkflowType> {
        if !self.auto_escalate {
            return None;
        }

        // 只在 Replan/Escalate 时升级
        if !matches!(repair_strategy, RepairStrategy::Replan | RepairStrategy::Escalate) {
            return None;
        }

        // 连续失败 2 次 → 升级
        if failure_count < 2 {
            return None;
        }

        match current {
            WorkflowType::ReAct => Some(WorkflowType::PlanExecute),
            WorkflowType::PlanExecute => Some(WorkflowType::MultiAgent),
            WorkflowType::MultiAgent | WorkflowType::Exploratory => None,
        }
    }
}

impl Default for DynamicOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}
