//! # Intelligence Metrics — 策略版本管理 & 使用追踪
//!
//! 参考 hermes-agent-rs 策略版本管理设计：
//! - canary rollout：新策略先灰度（10%流量）验证，再全量
//! - 硬门限回滚：成功率低于阈值立即回滚到上一版本
//! - 审计日志：所有策略变更可追溯、可审计

use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ── 模型定价 ─────────────────────────────────────────────────

static MODEL_PRICING: once_cell::sync::Lazy<HashMap<&'static str, ModelPricing>> =
    once_cell::sync::Lazy::new(|| {
        let mut m = HashMap::new();
        m.insert("gpt-4o", ModelPricing { input: 5.0, output: 15.0 });
        m.insert("gpt-4o-mini", ModelPricing { input: 0.15, output: 0.6 });
        m.insert("claude-3-5-sonnet", ModelPricing { input: 3.0, output: 15.0 });
        m.insert("claude-3-haiku", ModelPricing { input: 0.25, output: 1.25 });
        m.insert("deepseek-chat", ModelPricing { input: 0.1, output: 0.1 });
        m.insert("qwen-plus", ModelPricing { input: 0.2, output: 0.6 });
        m.insert("glm-4", ModelPricing { input: 0.1, output: 0.1 });
        m.insert("minimax", ModelPricing { input: 0.1, output: 0.1 });
        m
    });

#[derive(Debug, Clone, Copy)]
pub struct ModelPricing { pub input: f64, pub output: f64 }

// ── 使用记录 ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub task_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub total_cost_usd: f64,
    pub total_requests: usize,
    pub by_model: HashMap<String, ModelCost>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelCost {
    pub requests: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
}

// ── 使用追踪器 ────────────────────────────────────────────────

pub struct UsageTracker {
    records: Arc<RwLock<Vec<UsageRecord>>>,
    session_start: DateTime<Utc>,
    max_cost_usd: Option<f64>,
    degrade_at_ratio: f64,
}

impl UsageTracker {
    pub fn new() -> Self { Self {
        records: Arc::new(RwLock::new(Vec::new())),
        session_start: Utc::now(),
        max_cost_usd: None,
        degrade_at_ratio: 0.8,
    }}

    pub fn with_max_cost(mut self, max: f64) -> Self { self.max_cost_usd = Some(max); self }

    pub async fn record(&self, model: &str, input: usize, output: usize, latency_ms: u64, task_type: &str) -> Result<()> {
        let cost = self.calculate_cost(model, input, output);
        self.records.write().await.push(UsageRecord {
            timestamp: Utc::now(), model: model.into(),
            input_tokens: input, output_tokens: output,
            total_tokens: input + output, cost_usd: cost,
            latency_ms, task_type: task_type.into(),
        });
        Ok(())
    }

    pub async fn check_cost_gate(&self) -> Result<bool> {
        let summary = self.summarize().await;
        if let Some(max) = self.max_cost_usd {
            if summary.total_cost_usd >= max {
                tracing::warn!("💰 成本上限触发: ${:.4} / ${:.4}", summary.total_cost_usd, max);
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn should_degrade(&self) -> bool {
        if let Some(max) = self.max_cost_usd {
            let summary = self.summarize().await;
            summary.total_cost_usd / max >= self.degrade_at_ratio
        } else { false }
    }

    fn calculate_cost(&self, model: &str, input: usize, output: usize) -> f64 {
        let pricing = MODEL_PRICING.get(model).copied().unwrap_or(ModelPricing { input: 1.0, output: 5.0 });
        (input as f64 / 1_000_000.0) * pricing.input + (output as f64 / 1_000_000.0) * pricing.output
    }

    pub async fn summarize(&self) -> CostSummary {
        let records = self.records.read().await;
        let mut summary = CostSummary::default();
        for r in records.iter() {
            summary.total_input_tokens += r.input_tokens;
            summary.total_output_tokens += r.output_tokens;
            summary.total_cost_usd += r.cost_usd;
            summary.total_requests += 1;
            let entry = summary.by_model.entry(r.model.clone()).or_default();
            entry.requests += 1;
            entry.input_tokens += r.input_tokens;
            entry.output_tokens += r.output_tokens;
            entry.cost_usd += r.cost_usd;
        }
        summary
    }

    pub async fn reset(&self) { self.records.write().await.clear(); }
}

impl Default for UsageTracker { fn default() -> Self { Self::new() } }

// ── 策略版本管理 ──────────────────────────────────────────────

/// 策略版本元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersion {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub rollout_ratio: f64,          // 0.0-1.0，灰度比例
    pub rollout_status: RolloutStatus,
    pub trigger: String,            // 触发原因
    pub metrics_snapshot: PolicyMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RolloutStatus {
    /// 灰度验证中（canary）
    Canary,
    /// 已全量发布
    Full,
    /// 硬门限触发，已回滚
    RolledBack,
    /// 已废弃
    Deprecated,
}

/// 策略发布时的指标快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyMetrics {
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub avg_cost_per_call: f64,
    pub total_calls: usize,
}

// ── 策略版本管理器
///
/// hermes-agent-rs 核心设计：策略版本管理 + canary rollout + 硬门限回滚 + 审计日志
pub struct PolicyVersionManager {
    versions: Arc<RwLock<Vec<PolicyVersion>>>,
    audit_log: Arc<RwLock<Vec<AuditEntry>>>,
    db_path: PathBuf,
    /// 硬门限：成功率低于此值立即回滚
    hard_rollback_threshold: f64,
    /// 灰度验证最小样本数
    canary_min_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event: AuditEvent,
    pub version_from: Option<String>,
    pub version_to: Option<String>,
    pub reason: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEvent {
    VersionCreated,
    CanaryStarted,
    CanaryPromoted,
    HardRollbackTriggered,
    ManualRollback,
    Deprecated,
}

impl PolicyVersionManager {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            versions: Arc::new(RwLock::new(Vec::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            db_path,
            hard_rollback_threshold: 0.6,   // 成功率 < 60% 立即回滚
            canary_min_samples: 20,         // 灰度至少 20 样本才评估
        }
    }

    /// 初始化：从 SQLite 加载历史版本
    pub async fn init(&self) -> Result<()> {
        let conn = rusqlite::Connection::open(&self.db_path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS policy_versions (
                id TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                created_at TEXT NOT NULL,
                rollout_ratio REAL NOT NULL,
                rollout_status TEXT NOT NULL,
                trigger TEXT NOT NULL,
                metrics_snapshot TEXT NOT NULL
            )", [])?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS policy_audit_log (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                version_from TEXT,
                version_to TEXT,
                reason TEXT NOT NULL,
                metadata TEXT NOT NULL
            )", [])?;

        tracing::info!("策略版本管理器初始化完成 | db: {}", self.db_path.display());
        Ok(())
    }

    /// 发布新策略版本（canary rollout）
    ///
    /// 流程：创建版本(10%流量) → 验证 → 全量/回滚
    pub async fn publish_version(
        &self,
        version: String,
        metrics: PolicyMetrics,
        trigger: &str,
    ) -> Result<()> {
        let entry = PolicyVersion {
            version: version.clone(),
            created_at: Utc::now(),
            rollout_ratio: 0.1,          // 初始灰度 10%
            rollout_status: RolloutStatus::Canary,
            trigger: trigger.into(),
            metrics_snapshot: metrics.clone(),
        };

        // 写入内存
        self.versions.write().await.push(entry.clone());

        // 写入 SQLite
        let conn = rusqlite::Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO policy_versions (id, version, created_at, rollout_ratio, rollout_status, trigger, metrics_snapshot)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                entry.version,
                entry.created_at.to_rfc3339(),
                entry.rollout_ratio,
                serde_json::to_string(&RolloutStatus::Canary)?,
                entry.trigger,
                serde_json::to_string(&metrics)?,
            ],
        )?;

        // 写审计日志
        self.write_audit(AuditEvent::CanaryStarted, None, Some(&version), trigger).await?;

        tracing::info!(
            "策略版本发布 (canary 10%): v{} | 成功率 {:.1}%",
            version, metrics.success_rate * 100.0
        );

        Ok(())
    }

    /// 评估 canary 结果，决定是否全量或回滚
    ///
    /// hermes-agent-rs 硬门限回滚设计：
    /// 成功率 < hard_rollback_threshold → 立即回滚，不等待更多数据
    pub async fn evaluate_canary(&self, version: &str, live_metrics: &PolicyMetrics) -> Result<RolloutDecision> {
        let mut versions = self.versions.write().await;
        let entry = versions.iter_mut().find(|v| v.version == version);

        let entry = match entry {
            Some(e) => e,
            None => return Ok(RolloutDecision::UnknownVersion),
        };

        // 硬门限回滚检查（优先级最高）
        if live_metrics.success_rate < self.hard_rollback_threshold {
            entry.rollout_status = RolloutStatus::RolledBack;
            entry.rollout_ratio = 0.0;

            self.write_audit(
                AuditEvent::HardRollbackTriggered,
                Some(version),
                None,
                &format!("成功率 {:.1}% < 阈值 {:.1}%",
                    live_metrics.success_rate * 100.0,
                    self.hard_rollback_threshold * 100.0),
            ).await?;

            tracing::warn!(
                "🚨 硬门限回滚: v{} | 成功率 {:.1}% < {:.1}%",
                version, live_metrics.success_rate * 100.0, self.hard_rollback_threshold * 100.0
            );

            return Ok(RolloutDecision::HardRollback {
                version: version.into(),
                threshold: self.hard_rollback_threshold,
                actual: live_metrics.success_rate,
            });
        }

        // canary 最小样本检查
        if live_metrics.total_calls < self.canary_min_samples {
            return Ok(RolloutDecision::WaitingForMoreSamples {
                version: version.into(),
                current: live_metrics.total_calls,
                required: self.canary_min_samples,
            });
        }

        // 满足样本数后，看成功率是否达标
        let promote_threshold = self.hard_rollback_threshold + 0.15; // 75% 通过线
        if live_metrics.success_rate >= promote_threshold {
            entry.rollout_status = RolloutStatus::Full;
            entry.rollout_ratio = 1.0;

            self.write_audit(
                AuditEvent::CanaryPromoted,
                Some(version),
                Some(version),
                &format!("成功率 {:.1}% >= {:.1}%，全量发布", live_metrics.success_rate * 100.0, promote_threshold * 100.0),
            ).await?;

            tracing::info!("✅ Canary 验证通过，全量发布: v{}", version);

            Ok(RolloutDecision::Promoted { version: version.into() })
        } else {
            // 成功率在 60%-75% 之间，继续灰度
            entry.rollout_ratio = (entry.rollout_ratio + 0.1).min(0.5);
            tracing::info!("Canary 继续观察: v{} | 成功率 {:.1}%", version, live_metrics.success_rate * 100.0);
            Ok(RolloutDecision::CanaryContinued {
                version: version.into(),
                ratio: entry.rollout_ratio,
            })
        }
    }

    /// 手动回滚到指定版本
    pub async fn manual_rollback(&self, target_version: &str, reason: &str) -> Result<()> {
        let mut versions = self.versions.write().await;

        // 标记当前所有版本
        for v in versions.iter_mut() {
            if v.rollout_status == RolloutStatus::Full || v.rollout_status == RolloutStatus::Canary {
                v.rollout_status = RolloutStatus::RolledBack;
                v.rollout_ratio = 0.0;
            }
        }

        // 激活目标版本
        if let Some(target) = versions.iter_mut().find(|v| v.version == target_version) {
            target.rollout_status = RolloutStatus::Full;
            target.rollout_ratio = 1.0;
        }

        self.write_audit(
            AuditEvent::ManualRollback,
            None,
            Some(target_version),
            reason,
        ).await?;

        tracing::warn!("🔄 手动回滚到: v{}", target_version);
        Ok(())
    }

    /// 获取当前活跃的策略版本
    pub async fn active_version(&self) -> Option<String> {
        let versions = self.versions.read().await;
        versions.iter()
            .find(|v| v.rollout_status == RolloutStatus::Full)
            .map(|v| v.version.clone())
    }

    /// 列出所有历史版本
    pub async fn list_versions(&self) -> Vec<PolicyVersion> {
        self.versions.read().await.clone()
    }

    /// 获取审计日志
    pub async fn get_audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        let log = self.audit_log.read().await;
        log.iter().rev().take(limit).cloned().collect()
    }

    /// 写审计日志
    async fn write_audit(
        &self,
        event: AuditEvent,
        version_from: Option<&str>,
        version_to: Option<&str>,
        reason: &str,
    ) -> Result<()> {
        let entry = AuditEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event: event.clone(),
            version_from: version_from.map(String::from),
            version_to: version_to.map(String::from),
            reason: reason.into(),
            metadata: serde_json::json!({}),
        };

        self.audit_log.write().await.push(entry.clone());

        let conn = rusqlite::Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO policy_audit_log (id, timestamp, event_type, version_from, version_to, reason, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.id,
                entry.timestamp.to_rfc3339(),
                serde_json::to_string(&event)?,
                entry.version_from,
                entry.version_to,
                entry.reason,
                serde_json::to_string(&entry.metadata)?,
            ],
        )?;

        Ok(())
    }
}

/// 灰度评估结果
#[derive(Debug, Clone)]
pub enum RolloutDecision {
    /// 硬门限触发，立即回滚
    HardRollback { version: String, threshold: f64, actual: f64 },
    /// 样本不足，继续观察
    WaitingForMoreSamples { version: String, current: usize, required: usize },
    /// 验证通过，全量发布
    Promoted { version: String },
    /// 继续灰度
    CanaryContinued { version: String, ratio: f64 },
    /// 版本不存在
    UnknownVersion,
}

// ── 长任务规划器 ──────────────────────────────────────────────

pub struct LongTaskPlanner {
    long_task_threshold: usize,
    max_parallelism: usize,
    checkpoint_interval: usize,
}

impl LongTaskPlanner {
    pub fn new() -> Self { Self { long_task_threshold: 5000, max_parallelism: 4, checkpoint_interval: 3 } }

    pub fn analyze(&self, task: &str, available_tokens: usize) -> PlanningDecision {
        let task_len = task.len();
        let is_long = task_len > self.long_task_threshold || available_tokens > self.long_task_threshold;
        if !is_long {
            return PlanningDecision {
                needs_split: false, parallelism: 1, checkpoint_interval: 0,
                estimated_turns: 3, reasoning: "短任务，无需拆分".into()
            };
        }
        let parallelism = ((available_tokens as f64 / 3000.0).ceil() as usize).min(self.max_parallelism);
        PlanningDecision {
            needs_split: true, parallelism, checkpoint_interval: self.checkpoint_interval,
            estimated_turns: (available_tokens as f64 / 2000.0).ceil() as usize,
            reasoning: format!("长任务，建议并行度 {}", parallelism)
        }
    }
}

impl Default for LongTaskPlanner { fn default() -> Self { Self::new() } }

#[derive(Debug, Clone)]
pub struct PlanningDecision {
    pub needs_split: bool,
    pub parallelism: usize,
    pub checkpoint_interval: usize,
    pub estimated_turns: usize,
    pub reasoning: String,
}
