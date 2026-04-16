//! 模型路由器 — 根据任务特征、成本、延迟选择最优 LLM
//!
//! 路由策略：
//! - 代码生成 → DeepSeek-V3 / Qwen-Coder（成本最优）
//! - 中文推理 → GLM-4 / Qwen-Plus（中文场景）
//! - 超长上下文 → MiniMax-Text-01（1M 上下文）
//! - 最高质量 → GPT-4o / o3（关键决策）
//!
//! 参考 hermes-agent-rs Thompson Sampling 模型选择

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::config::LlmConfig;

/// 模型路由决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    /// 选中的模型
    pub model: String,
    /// 选中的 provider
    pub provider: String,
    /// 选择理由
    pub reason: String,
    /// 预估成本（USD）
    pub estimated_cost: f64,
    /// 预估延迟（ms）
    pub estimated_latency_ms: u64,
}

/// 模型能力画像
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub provider: String,
    pub context_window: usize,
    pub code_quality: f64,       // 0.0 - 1.0
    pub chinese_quality: f64,    // 0.0 - 1.0
    pub reasoning_quality: f64,  // 0.0 - 1.0
    pub speed_score: f64,        // 0.0 - 1.0
    pub cost_per_1k_input: f64,  // USD
    pub cost_per_1k_output: f64, // USD
}

/// 模型路由器
pub struct ModelRouter {
    profiles: Vec<ModelProfile>,
    /// Thompson Sampling 历史成功率（model → 成功次数/总次数）
    success_stats: Arc<RwLock<HashMap<String, (u32, u32)>>>,
}

impl ModelRouter {
    pub fn new(_config: &LlmConfig) -> Self {
        Self {
            profiles: Self::default_profiles(),
            success_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 默认模型画像
    fn default_profiles() -> Vec<ModelProfile> {
        vec![
            ModelProfile {
                id: "deepseek-chat".into(),
                provider: "deepseek".into(),
                context_window: 640_000,
                code_quality: 0.85,
                chinese_quality: 0.80,
                reasoning_quality: 0.82,
                speed_score: 0.90,
                cost_per_1k_input: 0.0001,
                cost_per_1k_output: 0.0001,
            },
            ModelProfile {
                id: "qwen-coder-plus".into(),
                provider: "dashscope".into(),
                context_window: 131_072,
                code_quality: 0.88,
                chinese_quality: 0.85,
                reasoning_quality: 0.80,
                speed_score: 0.85,
                cost_per_1k_input: 0.0002,
                cost_per_1k_output: 0.0006,
            },
            ModelProfile {
                id: "glm-4-plus".into(),
                provider: "zhipu".into(),
                context_window: 128_000,
                code_quality: 0.78,
                chinese_quality: 0.90,
                reasoning_quality: 0.82,
                speed_score: 0.88,
                cost_per_1k_input: 0.0001,
                cost_per_1k_output: 0.0001,
            },
            ModelProfile {
                id: "minimax-text-01".into(),
                provider: "minimax".into(),
                context_window: 1_000_000,
                code_quality: 0.75,
                chinese_quality: 0.80,
                reasoning_quality: 0.75,
                speed_score: 0.70,
                cost_per_1k_input: 0.0001,
                cost_per_1k_output: 0.0001,
            },
            ModelProfile {
                id: "gpt-4o".into(),
                provider: "openai".into(),
                context_window: 128_000,
                code_quality: 0.92,
                chinese_quality: 0.80,
                reasoning_quality: 0.95,
                speed_score: 0.75,
                cost_per_1k_input: 0.005,
                cost_per_1k_output: 0.015,
            },
            ModelProfile {
                id: "o3".into(),
                provider: "openai".into(),
                context_window: 200_000,
                code_quality: 0.90,
                chinese_quality: 0.78,
                reasoning_quality: 0.98,
                speed_score: 0.50,
                cost_per_1k_input: 0.003,
                cost_per_1k_output: 0.012,
            },
        ]
    }

    /// 根据任务特征路由模型
    pub async fn route(&self, task: &str, priority: RoutePriority) -> RouteDecision {
        let task_lower = task.to_lowercase();

        // 规则匹配（优先级高于 Thompson Sampling）
        let candidate = if priority == RoutePriority::Quality {
            // 最高质量 → GPT-4o / o3
            self.profiles.iter().find(|p| p.id == "gpt-4o")
        } else if task_lower.contains("超长") || task_lower.contains("大文档") || task_lower.contains("全库") {
            // 超长上下文 → MiniMax
            self.profiles.iter().find(|p| p.id == "minimax-text-01")
        } else if task_lower.contains("代码") || task_lower.contains("函数") || task_lower.contains("实现")
            || task_lower.contains("写一个") || task_lower.contains("生成代码")
        {
            // 代码生成 → DeepSeek / Qwen-Coder
            self.profiles.iter()
                .filter(|p| p.code_quality >= 0.85)
                .min_by(|a, b| a.cost_per_1k_input.partial_cmp(&b.cost_per_1k_input).unwrap())
        } else if task_lower.contains("分析") || task_lower.contains("推理") || task_lower.contains("决策") {
            // 推理 → GPT-4o
            self.profiles.iter().find(|p| p.reasoning_quality >= 0.95)
        } else {
            // 默认：成本最优
            self.profiles.iter()
                .min_by(|a, b| a.cost_per_1k_input.partial_cmp(&b.cost_per_1k_input).unwrap())
        };

        let profile = candidate.cloned().unwrap_or_else(|| self.profiles[0].clone());

        // Thompson Sampling 微调：如果某模型历史成功率显著低，降权
        let stats = self.success_stats.read().await;
        let adjusted_model = if let Some((success, total)) = stats.get(&profile.id) {
            if *total > 10 && (*success as f64 / *total as f64) < 0.5 {
                // 成功率低于 50%，切换到备选
                tracing::warn!("模型 {} 成功率过低 ({}/{})，切换备选", profile.id, success, total);
                self.profiles.iter()
                    .find(|p| p.id != profile.id && p.code_quality >= 0.80)
                    .cloned()
                    .unwrap_or(profile)
            } else {
                profile
            }
        } else {
            profile
        };

        RouteDecision {
            model: adjusted_model.id.clone(),
            provider: adjusted_model.provider.clone(),
            reason: self.build_reason(&adjusted_model, priority),
            estimated_cost: adjusted_model.cost_per_1k_input * 4.0 + adjusted_model.cost_per_1k_output * 2.0,
            estimated_latency_ms: (1.0 - adjusted_model.speed_score) as u64 * 3000 + 500,
        }
    }

    /// 记录模型使用结果（供 Thompson Sampling 更新）
    pub async fn record_outcome(&self, model: &str, success: bool) {
        let mut stats = self.success_stats.write().await;
        let entry = stats.entry(model.to_string()).or_insert((0, 0));
        if success { entry.0 += 1; }
        entry.1 += 1;
    }

    fn build_reason(&self, profile: &ModelProfile, priority: RoutePriority) -> String {
        match priority {
            RoutePriority::Cost => format!("成本优先: {} (${:.4}/1K input)", profile.id, profile.cost_per_1k_input),
            RoutePriority::Speed => format!("速度优先: {} (速度评分 {:.1})", profile.id, profile.speed_score),
            RoutePriority::Quality => format!("质量优先: {} (代码 {:.1}, 推理 {:.1})", profile.id, profile.code_quality, profile.reasoning_quality),
            RoutePriority::Balanced => format!("平衡选择: {} (综合评分 {:.1})", profile.id,
                (profile.code_quality + profile.reasoning_quality + profile.speed_score) / 3.0),
        }
    }
}

/// 路由优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutePriority {
    Cost,
    Speed,
    Quality,
    Balanced,
}
