//! CoT 推理模式 — Chain-of-Thought 选择器
//!
//! 三种模式：
//! - FORCED: 强制链式推理（复杂任务）
//! - FORCED_WITH_SELF_CRITIQUE: 推理 + 自我批评（高风险任务）
//! - SKIP: 跳过推理直接输出（简单任务）
//!
//! 参考 ReAct + CoT 论文：推理过程可审计、可回溯

use serde::{Deserialize, Serialize};

/// CoT 模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum CoTMode {
    /// 强制推理：所有回答必须先展示推理过程
    Forced,
    /// 推理 + 自我批评：推理后检查逻辑漏洞
    ForcedWithSelfCritique,
    /// 跳过推理：直接输出结果
    Skip,
}

/// CoT 选择器 — 根据任务特征自动选择推理模式
pub struct CoTSelector {
    /// 默认模式
    default_mode: CoTMode,
    /// 自我批评的阈值（任务复杂度评分 > 此值时启用）
    critique_threshold: f64,
}

impl CoTSelector {
    pub fn new() -> Self {
        Self {
            default_mode: CoTMode::Forced,
            critique_threshold: 0.7,
        }
    }

    /// 根据任务特征选择 CoT 模式
    pub fn select(&self, task: &str, complexity_score: f64) -> CoTMode {
        // 极简任务 → SKIP
        if complexity_score < 0.2 {
            return CoTMode::Skip;
        }

        // 高复杂度或高风险关键词 → 强制 + 自我批评
        let high_risk_keywords = [
            "删除", "迁移", "重构", "安全", "认证", "支付",
            "delete", "migrate", "refactor", "security", "auth", "payment",
        ];

        let is_high_risk = high_risk_keywords.iter()
            .any(|kw| task.to_lowercase().contains(kw));

        if is_high_risk || complexity_score > self.critique_threshold {
            CoTMode::ForcedWithSelfCritique
        } else {
            self.default_mode
        }
    }

    /// 生成 CoT 提示词
    pub fn build_prompt(&self, mode: CoTMode, task: &str) -> String {
        match mode {
            CoTMode::Forced => format!(
                "请逐步推理以下任务，展示完整的思考过程：\n\n{}\n\n\
                请按以下格式回答：\n\
                1. 思考：分析任务需求和约束\n\
                2. 推理：逐步推导解决方案\n\
                3. 方案：给出具体实施步骤",
                task
            ),
            CoTMode::ForcedWithSelfCritique => format!(
                "请逐步推理以下任务，并在推理后进行自我批评：\n\n{}\n\n\
                请按以下格式回答：\n\
                1. 思考：分析任务需求和约束\n\
                2. 推理：逐步推导解决方案\n\
                3. 自我批评：检查推理中的逻辑漏洞和假设\n\
                4. 修正：根据批评修正方案\n\
                5. 最终方案：给出具体实施步骤",
                task
            ),
            CoTMode::Skip => task.to_string(),
        }
    }

    /// 从 LLM 响应中提取推理链
    pub fn extract_chain(response: &str) -> Vec<ReasoningStep> {
        let mut steps = Vec::new();
        let mut current_step = ReasoningStep::default();

        for line in response.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("1. 思考") || trimmed.starts_with("思考：") {
                if !current_step.content.is_empty() {
                    steps.push(current_step.clone());
                }
                current_step = ReasoningStep {
                    step_type: StepType::Thinking,
                    content: trimmed.to_string(),
                    confidence: None,
                };
            } else if trimmed.starts_with("2. 推理") || trimmed.starts_with("推理：") {
                if !current_step.content.is_empty() {
                    steps.push(current_step.clone());
                }
                current_step = ReasoningStep {
                    step_type: StepType::Reasoning,
                    content: trimmed.to_string(),
                    confidence: None,
                };
            } else if trimmed.starts_with("3. 自我批评") || trimmed.starts_with("自我批评：") {
                if !current_step.content.is_empty() {
                    steps.push(current_step.clone());
                }
                current_step = ReasoningStep {
                    step_type: StepType::SelfCritique,
                    content: trimmed.to_string(),
                    confidence: None,
                };
            } else if trimmed.starts_with("5. 最终方案") || trimmed.starts_with("方案：") {
                if !current_step.content.is_empty() {
                    steps.push(current_step.clone());
                }
                current_step = ReasoningStep {
                    step_type: StepType::FinalPlan,
                    content: trimmed.to_string(),
                    confidence: None,
                };
            } else if !trimmed.is_empty() {
                current_step.content.push_str("\n");
                current_step.content.push_str(trimmed);
            }
        }

        if !current_step.content.is_empty() {
            steps.push(current_step);
        }

        steps
    }
}

impl Default for CoTSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// 推理步骤
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReasoningStep {
    pub step_type: StepType,
    pub content: String,
    pub confidence: Option<f64>,
}

/// 推理步骤类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    #[default]
    Thinking,
    Reasoning,
    SelfCritique,
    Correction,
    FinalPlan,
}
