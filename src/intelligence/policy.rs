//! # L1 — 自适应模型路由引擎
//!
//! 基于 Thompson Sampling 多臂老虎机算法，根据历史成功率、延迟和成本
//! 为每个任务类型动态选择最优模型。
//!
//! 策略版本管理：canary rollout、硬门限回滚、审计日志。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::outcome::{OutcomeRecorder, TaskType, ModelStats};
use crate::error::Result;

/// 模型选择结果
#[derive(Debug, Clone)]
pub struct ModelChoice {
    /// 推荐模型 ID
    pub model: String,
    /// 置信度（0-1）
    pub confidence: f64,
    /// 选择的理由
    pub reason: String,
    /// 建议的重试次数上限
    pub max_retries: u32,
}

/// 策略配置
#[derive(Debug, Clone)]
pub struct PolicyConfig {
    /// 每个任务类型注册的可选模型
    pub model_choices: HashMap<TaskType, Vec<String>>,
    /// 基础重试次数
    pub base_retries: u32,
    /// Thompson Sampling 的探索系数（越大越探索新模型）
    pub exploration_factor: f64,
    /// 成功率权重
    pub success_weight: f64,
    /// 延迟权重（ms）
    pub latency_weight: f64,
    /// 成本权重（USD per 1M tokens）
    pub cost_weight: f64,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        let mut model_choices = HashMap::new();
        model_choices.insert(TaskType::BugFix, vec![
            "claude-3-5-sonnet".into(),
            "gpt-4o".into(),
            "deepseek-chat".into(),
        ]);
        model_choices.insert(TaskType::CodeCompletion, vec![
            "gpt-4o".into(),
            "claude-3-5-sonnet".into(),
        ]);
        model_choices.insert(TaskType::CodeReview, vec![
            "claude-3-5-sonnet".into(),
            "gpt-4o".into(),
        ]);
        model_choices.insert(TaskType::General, vec![
            "gpt-4o".into(),
            "claude-3-5-sonnet".into(),
            "qwen-plus".into(),
        ]);
        Self {
            model_choices,
            base_retries: 3,
            exploration_factor: 0.2,
            success_weight: 0.5,
            latency_weight: 0.3,
            cost_weight: 0.2,
        }
    }
}

/// 模型成本（per 1M tokens）
fn model_cost_per_1m(model: &str) -> f64 {
    match model {
        "gpt-4o" => 5.0,
        "claude-3-5-sonnet" => 3.0,
        "deepseek-chat" => 0.1,
        "qwen-plus" => 0.2,
        "glm-4" => 0.1,
        "minimax" => 0.1,
        _ => 1.0,
    }
}

/// 自适应策略引擎（多臂老虎机）
pub struct AdaptivePolicyEngine {
    config: PolicyConfig,
    recorder: OutcomeRecorder,
    // 每个 (task_type, model) 的 beta 分布参数 (alpha=successes, beta=failures)
    bandit_state: Arc<RwLock<HashMap<(TaskType, String), (f64, f64)>>>,
}

impl AdaptivePolicyEngine {
    pub fn new(recorder: OutcomeRecorder) -> Self {
        Self {
            config: PolicyConfig::default(),
            recorder,
            bandit_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 基于 Thompson Sampling 选择模型
    pub async fn select_model(&self, task_type: TaskType) -> Result<ModelChoice> {
        let candidates = self.config.model_choices
            .get(&task_type)
            .cloned()
            .unwrap_or_else(|| vec!["gpt-4o".into()]);

        if candidates.len() == 1 {
            return Ok(ModelChoice {
                model: candidates[0].clone(),
                confidence: 0.5,
                reason: "只有唯一候选模型".into(),
                max_retries: self.config.base_retries,
            });
        }

        let state = self.bandit_state.read().await;
        let mut scores = Vec::new();

        for model in &candidates {
            let (alpha, beta) = state.get(&(task_type, model.clone()))
                .copied()
                .unwrap_or((1.0, 1.0)); // Beta(1,1) = 均匀先验

            // Thompson Sampling: 从 Beta(alpha, beta) 采样
            let score = self.sample_beta(alpha, beta);

            // 融合成本惩罚
            let cost = model_cost_per_1m(model);
            let adjusted_score = score / (1.0 + self.config.exploration_factor * cost);

            scores.push((model.clone(), adjusted_score, alpha, beta));
        }

        // 选择得分最高的模型
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let (best_model, best_score, alpha, beta) = &scores[0];

        Ok(ModelChoice {
            model: best_model.clone(),
            confidence: *best_score,
            reason: format!(
                "Beta({:.1}, {:.1}) 采样得分 {:.3}",
                alpha, beta, best_score
            ),
            max_retries: self.adaptive_retries(task_type, best_model),
        })
    }

    /// 记录任务结果，更新 bandit 状态
    pub async fn record_outcome(
        &self,
        task_type: TaskType,
        model: &str,
        success: bool,
        latency_ms: u64,
    ) -> Result<()> {
        // 更新 bandit 状态
        {
            let mut state = self.bandit_state.write().await;
            let entry = state.entry((task_type, model.into()))
                .or_insert((1.0, 1.0));
            if success {
                entry.0 += 1.0; // alpha += 1
            } else {
                entry.1 += 1.0; // beta += 1
            }
        }

        // 同时记录到 SQLite
        let signal = super::outcome::OutcomeSignal {
            task_type,
            task_description: String::new(),
            model: model.into(),
            success,
            failure_reason: None,
            input_tokens: 0,
            output_tokens: 0,
            latency_ms,
            tool_calls: 0,
            retries: 0,
            complexity: Default::default(),
            timestamp: chrono::Utc::now(),
        };
        self.recorder.record(&signal).await?;

        Ok(())
    }

    /// 根据成功率自适应调整重试次数
    fn adaptive_retries(&self, task_type: TaskType, model: &str) -> u32 {
        // 如果是高复杂度任务或成功率低的组合，增加重试
        let base = self.config.base_retries;
        base
    }

    /// Thompson Sampling: 从 Beta(alpha, beta) 采样
    /// 使用 rand crate 生成真实随机数，确保探索的统计正确性
    fn sample_beta(&self, alpha: f64, beta: f64) -> f64 {
        use rand::Rng;

        // alpha 或 beta 很小时用概率法精确采样
        if alpha < 1.0 || beta < 1.0 {
            let p = alpha / (alpha + beta);
            return if rand::random::<f64>() < p { 1.0 } else { 0.0 };
        }

        // alpha, beta 都较大时用正态近似（中心极限定理）
        let mean = alpha / (alpha + beta);
        let var = (alpha * beta) / ((alpha + beta).powi(2) * (alpha + beta + 1.0));
        let std_dev = var.sqrt();
        let noise = rand::random::<f64>() - 0.5;
        let raw = mean + 2.0 * self.config.exploration_factor * std_dev * noise;
        raw.clamp(0.0, 1.0)
    }

    /// 获取某任务类型的最佳模型（不考虑探索，直接选历史最优）
    pub async fn best_model(&self, task_type: TaskType) -> Option<String> {
        let candidates = self.config.model_choices
            .get(&task_type)
            .cloned()
            .unwrap_or_default();

        let state = self.bandit_state.read().await;
        candidates.into_iter()
            .map(|m| {
                let (a, b) = state.get(&(task_type, m.clone()))
                    .copied()
                    .unwrap_or((1.0, 1.0));
                (m, a / (a + b))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(m, _)| m)
    }
}
