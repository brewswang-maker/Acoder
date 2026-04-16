//! Skill 自进化引擎
//!
//! hermes-agent-rs 自进化引擎核心设计（L1/L2/L3 三层闭环）：
//!
//! ## L1 — 模型与重试调优
//! Thompson Sampling 根据历史成功率、延迟、成本为每类任务选最佳模型+重试策略。
//!
//! ## L2 — 长任务规划
//! 接收长任务自动决定并行度、子任务拆分、检查点间隔。
//!
//! ## L3 — Prompt 与记忆塑形
//! 每次请求后根据 OutcomeSignal 优化系统提示词和上下文长度。
//!
//! ## 反馈闭环
//! OutcomeSignal → 性能评估 → 归因分析 → 改进生成 → 三层评估 → 上线/回滚
//!              ↑___________ PolicyVersionManager (canary + hard rollback) ___________|

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::intelligence::outcome::{OutcomeSignal, TaskType, ComplexityLevel};
use crate::intelligence::metrics::{PolicyVersionManager, PolicyMetrics, RolloutDecision};

/// 失败案例记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureCase {
    pub task_description: String,
    pub task_type: TaskType,
    pub skill_id: String,
    pub failure_reason: String,
    pub timestamp: DateTime<Utc>,
    /// 归因结果
    pub attribution: FailureAttribution,
    /// 是否已处理
    pub resolved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureAttribution {
    /// Skill 本身质量不足，需要优化指令
    SkillQuality,
    /// 任务与 Skill 不匹配，需要重新选型
    TaskMismatch,
    /// 外部依赖失败（网络、环境），不需要改 Skill
    ExternalFailure,
    /// 模型能力不足，需要换更强的模型
    ModelInsufficiency,
    /// 任务复杂度超预期，需要拆分
    ComplexityMismatch,
}

/// 改进方案
#[derive(Debug, Clone)]
pub struct Improvement {
    pub skill_id: String,
    /// 改进类型
    pub improvement_type: ImprovementType,
    /// 改进描述
    pub description: String,
    /// 改进后的 prompt 片段（用于 L3 prompt shaping）
    pub prompt_delta: String,
    /// 置信度（0-1）
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImprovementType {
    PromptRefinement,   // L3: prompt 片段微调
    ContextLength,       // L3: 上下文长度调整
    ToolSelection,       // L2: 工具选择策略
    ModelSwitch,         // L1: 模型切换
}

/// Skill 性能报告
#[derive(Debug, Clone)]
pub struct SkillPerformance {
    pub skill_id: String,
    pub total_runs: usize,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub avg_tokens: f64,
    pub failure_attributions: HashMap<FailureAttribution, usize>,
    pub last_improved: Option<DateTime<Utc>>,
}

/// 进化引擎
pub struct EvolutionEngine {
    /// SQLite 数据库路径
    db_path: PathBuf,
    /// 策略版本管理器（接入 canary + hard rollback）
    policy_manager: Arc<RwLock<Option<PolicyVersionManager>>>,
    /// 失败案例内存缓存
    failure_cache: Arc<RwLock<Vec<FailureCase>>>,
    /// Skill 性能缓存
    performance_cache: Arc<RwLock<HashMap<String, SkillPerformance>>>,
    /// LLM 客户端（用于生成改进方案）
    llm: Option<crate::llm::Client>,
    /// 自动进化开关
    auto_evolve: bool,
    /// 自动进化阈值（成功率低于此值触发进化）
    auto_evolve_threshold: f64,
}

impl EvolutionEngine {
    /// 创建进化引擎
    pub fn new(db_path: PathBuf, auto_evolve: bool) -> Self {
        Self {
            db_path,
            policy_manager: Arc::new(RwLock::new(None)),
            failure_cache: Arc::new(RwLock::new(Vec::new())),
            performance_cache: Arc::new(RwLock::new(HashMap::new())),
            llm: None,
            auto_evolve,
            auto_evolve_threshold: 0.7,
        }
    }

    /// 设置 LLM 客户端（用于生成改进方案）
    pub fn with_llm(mut self, llm: crate::llm::Client) -> Self {
        self.llm = Some(llm);
        self
    }

    /// 初始化：连接策略版本管理器
    pub async fn init(&self) -> Result<()> {
        let pm = PolicyVersionManager::new(self.db_path.join("policy_versions.db"));
        pm.init().await?;
        *self.policy_manager.write().await = Some(pm);
        tracing::info!("自进化引擎初始化完成 | auto_evolve={}", self.auto_evolve);
        Ok(())
    }

    /// ── 反馈入口：每次任务完成后调用 ──────────────────────────
    ///
    /// 接收 OutcomeSignal → 记录 → 归因 → 评估 → 触发进化（可选）
    pub async fn on_outcome(&self, signal: &OutcomeSignal) -> Result<()> {
        tracing::debug!("收到 OutcomeSignal: {} | 成功={} | 模型={}",
            signal.task_type, signal.success, signal.model);

        // Step 1: 记录到数据库
        self.record_outcome(signal).await?;

        // Step 2: 如果失败，归因分析
        if !signal.success {
            self.analyze_failure(signal).await?;
        }

        // Step 3: 更新性能缓存
        self.update_performance(signal).await?;

        // Step 4: 检查是否需要自动进化（L1/L2/L3 联动）
        if self.auto_evolve {
            self.check_auto_evolve(signal).await?;
        }

        Ok(())
    }

    /// 手动触发 Skill 进化
    pub async fn evolve_skill(&self, skill_id: &str) -> Result<()> {
        tracing::info!("触发 Skill 进化: {}", skill_id);

        // Step 1: 检测当前 Skill 性能
        let perf = self.get_performance(skill_id).await?;
        tracing::info!("Skill {} 性能: 成功率 {:.1}% | 平均延迟 {:.0}ms",
            skill_id, perf.success_rate * 100.0, perf.avg_latency_ms);

        // Step 2: 收集失败案例
        let failures = self.get_unresolved_failures(skill_id).await?;
        tracing::info!("收集到 {} 个未解决失败案例", failures.len());

        if failures.is_empty() {
            tracing::info!("Skill {} 无失败案例，无需进化", skill_id);
            return Ok(());
        }

        // Step 3: 生成改进方案（L3 Prompt Shaping）
        let improvements = self.generate_improvements(skill_id, &failures, &perf).await?;
        tracing::info!("生成 {} 个改进方案", improvements.len());

        // Step 4: 发布新策略版本（canary rollout）
        let version = format!("v{}_{}", Utc::now().format("%Y%m%d_%H%M%S"), skill_id);
        let metrics = PolicyMetrics {
            success_rate: perf.success_rate,
            avg_latency_ms: perf.avg_latency_ms,
            avg_cost_per_call: perf.avg_tokens / 1_000_000.0 * 0.1, // 估算
            total_calls: perf.total_runs,
        };

        let pm_guard = self.policy_manager.read().await;
        if let Some(ref pm) = *pm_guard {
            let _: () = pm.publish_version(version.clone(), metrics.clone(), &format!("Skill {} 进化", skill_id)).await?;
        }

        // Step 5: 三层评估（hermes 核心安全保障）
        // 评估期间新版本只服务 10% 流量（canary）
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await; // 等待样本积累

        // 模拟实时评估（实际应从 metrics 服务拉取）
        let live_metrics = metrics; // 简化：直接用当前指标
        if let Some(ref pm) = *pm_guard {
            let decision: RolloutDecision = pm.evaluate_canary(&version, &live_metrics).await?;
            match decision {
                RolloutDecision::HardRollback { .. } => {
                    tracing::warn!("🚨 新版本 {} 硬门限回滚", version);
                    return Ok(());
                }
                RolloutDecision::Promoted { version } => {
                    tracing::info!("✅ 新版本 {} 全量发布", version);
                }
                RolloutDecision::CanaryContinued { version, ratio } => {
                    tracing::info!("继续灰度: {} | 流量比例 {:.0}%", version, ratio * 100.0);
                }
                RolloutDecision::WaitingForMoreSamples { version, current, required } => {
                    tracing::info!("等待样本: {} | {}/{}", version, current, required);
                }
                _ => {}
            }
        }

        // Step 6: 标记失败案例已处理
        self.mark_failures_resolved(&failures).await?;

        tracing::info!("Skill {} 进化流程完成", skill_id);
        Ok(())
    }

    // ── 内部方法 ─────────────────────────────────────────────

    /// 记录 OutcomeSignal 到 SQLite
    async fn record_outcome(&self, signal: &OutcomeSignal) -> Result<()> {
        let conn = rusqlite::Connection::open(&self.db_path.join("outcomes.db"))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS outcomes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_type TEXT NOT NULL, task_description TEXT NOT NULL,
                model TEXT NOT NULL, success INTEGER NOT NULL,
                failure_reason TEXT, input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL, latency_ms INTEGER NOT NULL,
                tool_calls INTEGER NOT NULL, retries INTEGER NOT NULL,
                complexity TEXT NOT NULL, timestamp TEXT NOT NULL
            )", [],
        )?;
        conn.execute(
            "INSERT INTO outcomes (task_type, task_description, model, success, failure_reason,
                input_tokens, output_tokens, latency_ms, tool_calls, retries, complexity, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                serde_json::to_string(&signal.task_type)?,
                signal.task_description, signal.model,
                signal.success as i32, signal.failure_reason,
                signal.input_tokens, signal.output_tokens,
                signal.latency_ms, signal.tool_calls, signal.retries,
                serde_json::to_string(&signal.complexity)?,
                signal.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 失败归因分析
    async fn analyze_failure(&self, signal: &OutcomeSignal) -> Result<()> {
        let reason = signal.failure_reason.as_deref().unwrap_or("未知");
        let attribution = self. attribute_failure(reason, signal);

        let case = FailureCase {
            task_description: signal.task_description.clone(),
            task_type: signal.task_type,
            skill_id: signal.model.clone(), // 简化：model 作为 skill_id
            failure_reason: reason.into(),
            timestamp: signal.timestamp,
            attribution,
            resolved: false,
        };

        self.failure_cache.write().await.push(case.clone());

        tracing::debug!("失败归因: {:?} | 原因: {}", attribution, reason);

        // 外部失败不触发进化
        if attribution == FailureAttribution::ExternalFailure {
            tracing::debug!("外部失败，无需进化");
        }

        Ok(())
    }

    /// 归因判断
    fn attribute_failure(&self, reason: &str, signal: &OutcomeSignal) -> FailureAttribution {
        let reason_lower = reason.to_lowercase();

        // 外部失败（网络、环境、超时）
        if reason_lower.contains("timeout")
            || reason_lower.contains("connection")
            || reason_lower.contains("network")
            || reason_lower.contains("rate limit")
            || reason_lower.contains("限流")
            || reason_lower.contains("网络") {
            return FailureAttribution::ExternalFailure;
        }

        // 模型能力不足
        if reason_lower.contains("model")
            || reason_lower.contains("context length")
            || reason_lower.contains("max token")
            || reason_lower.contains("context window") {
            return FailureAttribution::ModelInsufficiency;
        }

        // 复杂度超预期（工具调用过多 / token 超限）
        if signal.tool_calls > 15 || signal.input_tokens > 10000 {
            return FailureAttribution::ComplexityMismatch;
        }

        // Skill 质量问题 vs 任务不匹配
        // 启发式：成功率低于 50% 的 Skill 倾向于质量不足
        // 高复杂度任务 + 低成功率 → 任务不匹配
        if signal.complexity == ComplexityLevel::High {
            return FailureAttribution::TaskMismatch;
        }

        FailureAttribution::SkillQuality
    }

    /// 更新性能缓存
    async fn update_performance(&self, signal: &OutcomeSignal) -> Result<()> {
        let skill_id = &signal.model; // 简化
        let mut cache = self.performance_cache.write().await;

        let perf = cache.entry(skill_id.clone()).or_insert_with(|| SkillPerformance {
            skill_id: skill_id.clone(),
            total_runs: 0,
            success_rate: 0.0,
            avg_latency_ms: 0.0,
            avg_tokens: 0.0,
            failure_attributions: HashMap::new(),
            last_improved: None,
        });

        let n = perf.total_runs as f64;
        perf.success_rate = (perf.success_rate * n + if signal.success { 1.0 } else { 0.0 }) / (n + 1.0);
        perf.avg_latency_ms = (perf.avg_latency_ms * n + signal.latency_ms as f64) / (n + 1.0);
        perf.avg_tokens = (perf.avg_tokens * n + (signal.input_tokens + signal.output_tokens) as f64) / (n + 1.0);
        perf.total_runs += 1;

        Ok(())
    }

    /// 检查是否需要自动进化
    async fn check_auto_evolve(&self, signal: &OutcomeSignal) -> Result<()> {
        let perf = self.get_performance(&signal.model).await?;

        if perf.success_rate < self.auto_evolve_threshold && perf.total_runs >= 10 {
            tracing::warn!(
                "⚡ 自动触发进化: {} | 成功率 {:.1}% < {:.1}% | 样本数 {}",
                signal.model, perf.success_rate * 100.0,
                self.auto_evolve_threshold * 100.0, perf.total_runs
            );
            self.evolve_skill(&signal.model).await?;
        }

        Ok(())
    }

    /// 获取 Skill 性能
    async fn get_performance(&self, skill_id: &str) -> Result<SkillPerformance> {
        let cache = self.performance_cache.read().await;
        Ok(cache.get(skill_id).cloned().unwrap_or_else(|| SkillPerformance {
            skill_id: skill_id.into(),
            total_runs: 0,
            success_rate: 0.0,
            avg_latency_ms: 0.0,
            avg_tokens: 0.0,
            failure_attributions: HashMap::new(),
            last_improved: None,
        }))
    }

    /// 获取未解决的失败案例
    async fn get_unresolved_failures(&self, skill_id: &str) -> Result<Vec<FailureCase>> {
        let cache = self.failure_cache.read().await;
        Ok(cache.iter()
            .filter(|f| f.skill_id == skill_id && !f.resolved)
            .cloned()
            .collect())
    }

    /// 标记失败案例已解决
    async fn mark_failures_resolved(&self, failures: &[FailureCase]) -> Result<()> {
        let mut cache = self.failure_cache.write().await;
        for f in failures {
            if let Some(cached) = cache.iter_mut().find(|c| c.task_description == f.task_description && c.timestamp == f.timestamp) {
                cached.resolved = true;
            }
        }
        Ok(())
    }

    /// 生成改进方案（L3 Prompt Shaping）
    async fn generate_improvements(
        &self,
        skill_id: &str,
        failures: &[FailureCase],
        perf: &SkillPerformance,
    ) -> Result<Vec<Improvement>> {
        // LLM 生成改进（如果有 LLM）
        if let Some(ref llm) = self.llm {
            return self.generate_improvements_with_llm(llm, skill_id, failures, perf).await;
        }

        // 无 LLM 时用启发式生成
        Ok(self.generate_improvements_heuristic(skill_id, failures, perf))
    }

    /// 启发式改进方案（无 LLM 时使用）
    fn generate_improvements_heuristic(
        &self,
        skill_id: &str,
        failures: &[FailureCase],
        perf: &SkillPerformance,
    ) -> Vec<Improvement> {
        let mut improvements = Vec::new();

        // L3: Prompt 细化（成功率低于阈值时）
        if perf.success_rate < 0.8 {
            improvements.push(Improvement {
                skill_id: skill_id.into(),
                improvement_type: ImprovementType::PromptRefinement,
                description: "成功率偏低，建议细化 prompt 指令".into(),
                prompt_delta: "增加更具体的验收标准说明".into(),
                confidence: perf.success_rate,
            });
        }

        // L3: 上下文长度调整（token 用量高时）
        if perf.avg_tokens > 8000.0 {
            improvements.push(Improvement {
                skill_id: skill_id.into(),
                improvement_type: ImprovementType::ContextLength,
                description: "上下文长度较长，建议精简".into(),
                prompt_delta: "减少示例数量，保持核心指令简洁".into(),
                confidence: 0.7,
            });
        }

        // 统计失败归因分布
        let mut attr_counts = HashMap::new();
        for f in failures {
            *attr_counts.entry(f.attribution).or_insert(0) += 1;
        }

        // L1: 模型切换（模型能力不足时）
        if let Some(&count) = attr_counts.get(&FailureAttribution::ModelInsufficiency) {
            if count >= failures.len() / 2 {
                improvements.push(Improvement {
                    skill_id: skill_id.into(),
                    improvement_type: ImprovementType::ModelSwitch,
                    description: "模型能力不足，建议切换到更强模型".into(),
                    prompt_delta: "".into(),
                    confidence: 0.9,
                });
            }
        }

        // L2: 工具选择（复杂度超预期时）
        if let Some(&count) = attr_counts.get(&FailureAttribution::ComplexityMismatch) {
            if count >= 1 {
                improvements.push(Improvement {
                    skill_id: skill_id.into(),
                    improvement_type: ImprovementType::ToolSelection,
                    description: "任务复杂度超预期，建议优化任务拆分".into(),
                    prompt_delta: "将复杂任务拆分为多个简单步骤".into(),
                    confidence: 0.75,
                });
            }
        }

        improvements
    }

    /// LLM 驱动的改进方案生成
    async fn generate_improvements_with_llm(
        &self,
        llm: &crate::llm::Client,
        skill_id: &str,
        failures: &[FailureCase],
        perf: &SkillPerformance,
    ) -> Result<Vec<Improvement>> {
        let failure_summaries: Vec<_> = failures.iter().map(|f| {
            format!("- [{:?}] {}: {}", f.attribution, f.task_description, f.failure_reason)
        }).collect();

        let prompt = format!(r#"
你是 Skill 自进化引擎。基于以下失败案例，生成改进方案。

Skill ID: {}
成功率: {:.1}%
平均延迟: {:.0}ms
平均Token: {:.0}

失败案例:
{}

请生成改进建议，格式为 JSON 数组：
[
  {{
    "improvement_type": "prompt_refinement / context_length / tool_selection / model_switch",
    "description": "改进描述",
    "prompt_delta": "prompt 改动内容（仅 prompt_refinement 和 context_length 需要）",
    "confidence": 0.0-1.0
  }}
]

只输出 JSON 数组，不要有其他文字。
"#, skill_id, perf.success_rate * 100.0, perf.avg_latency_ms, perf.avg_tokens,
           failure_summaries.join("\n"));

        let messages = vec![
            crate::llm::Message::system("你是 Skill 自进化引擎，输出 JSON 数组格式的改进建议。"),
            crate::llm::Message::user(&prompt),
        ];

        let request = crate::llm::LlmRequest {
            model: "auto".into(),
            messages,
            temperature: Some(0.3),
            max_tokens: Some(2048),
            stream: false,
            tools: None,
        };

        let response = llm.complete(request).await
            .map_err(|e| crate::Error::LlmFailed { reason: e.to_string() })?;

        let suggestions: Vec<serde_json::Value> = serde_json::from_str(&response.content)
            .unwrap_or_default();

        let improvements: Vec<Improvement> = suggestions.iter().filter_map(|s| {
            let ty_str = s.get("improvement_type")?.as_str()?;
            let improvement_type = match ty_str {
                "prompt_refinement" => ImprovementType::PromptRefinement,
                "context_length" => ImprovementType::ContextLength,
                "tool_selection" => ImprovementType::ToolSelection,
                "model_switch" => ImprovementType::ModelSwitch,
                _ => return None,
            };
            Some(Improvement {
                skill_id: skill_id.into(),
                improvement_type,
                description: s.get("description")?.as_str()?.into(),
                prompt_delta: s.get("prompt_delta")?.as_str().unwrap_or("").into(),
                confidence: s.get("confidence")?.as_f64().unwrap_or(0.5),
            })
        }).collect();

        Ok(improvements)
    }
}

impl Default for EvolutionEngine {
    fn default() -> Self {
        Self::new(PathBuf::from("."), false)
    }
}
