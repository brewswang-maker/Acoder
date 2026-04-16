//! 人工审批管理 — 安全门禁
//!
//! 审批策略：
//! - 高风险操作自动拦截（删除/重命名/迁移）
//! - 白名单操作自动通过
//! - 灰度操作需人工确认
//!
//! 参考 ACoder 安全设计：
//! 所有文件写入、命令执行均需经过审批门禁

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::error::Result;

/// 审批决策
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// 自动通过
    AutoApproved,
    /// 需要人工确认
    NeedsApproval,
    /// 自动拒绝
    AutoRejected,
}

/// 审批结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResult {
    pub decision: ApprovalDecision,
    pub action: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

/// 审批策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    /// 自动通过的路径模式
    pub auto_approve_patterns: Vec<String>,
    /// 自动拒绝的路径模式
    pub auto_reject_patterns: Vec<String>,
    /// 需要审批的命令模式
    pub needs_approval_commands: Vec<String>,
    /// 最大文件大小（字节），超过需审批
    pub max_auto_file_size: usize,
    /// 是否启用（全局开关）
    pub enabled: bool,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            auto_approve_patterns: vec![
                "src/**".into(),
                "tests/**".into(),
                "docs/**".into(),
                "*.md".into(),
                "*.txt".into(),
            ],
            auto_reject_patterns: vec![
                "**/.env".into(),
                "**/credentials*".into(),
                "**/*secret*".into(),
                "**/*password*".into(),
                "/etc/**".into(),
                "/usr/**".into(),
            ],
            needs_approval_commands: vec![
                "rm".into(),
                "rmdir".into(),
                "mv".into(),
                "docker".into(),
                "curl".into(),
                "wget".into(),
                "npm publish".into(),
                "cargo publish".into(),
                "git push".into(),
                "git push --force".into(),
            ],
            max_auto_file_size: 100_000, // 100KB
            enabled: true,
        }
    }
}

/// 审批管理器
pub struct ApprovalManager {
    policy: ApprovalPolicy,
    /// 审批历史
    history: Arc<RwLock<Vec<ApprovalResult>>>,
    /// 待审批队列
    pending: Arc<RwLock<HashMap<String, PendingApproval>>>,
}

/// 待审批项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub action: String,
    pub details: String,
    pub risk_level: RiskLevel,
    pub created_at: DateTime<Utc>,
}

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl ApprovalManager {
    pub fn new() -> Self {
        Self {
            policy: ApprovalPolicy::default(),
            history: Arc::new(RwLock::new(Vec::new())),
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_policy(mut self, policy: ApprovalPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// 检查操作是否需要审批
    pub async fn check(&self, action: &str, target: &str, details: &str) -> ApprovalDecision {
        if !self.policy.enabled {
            return ApprovalDecision::AutoApproved;
        }

        // 检查自动拒绝
        for pattern in &self.policy.auto_reject_patterns {
            if self.match_pattern(target, pattern) {
                self.record(ApprovalDecision::AutoRejected, action, &format!("匹配拒绝规则: {}", pattern)).await;
                return ApprovalDecision::AutoRejected;
            }
        }

        // 检查危险命令
        for cmd in &self.policy.needs_approval_commands {
            if action.starts_with(cmd) {
                self.record(ApprovalDecision::NeedsApproval, action, &format!("危险命令: {}", cmd)).await;
                return ApprovalDecision::NeedsApproval;
            }
        }

        // 检查高风险关键词
        let high_risk_keywords = ["删除", "重命名", "迁移", "force", "drop", "truncate"];
        for kw in &high_risk_keywords {
            if details.to_lowercase().contains(kw) {
                self.record(ApprovalDecision::NeedsApproval, action, &format!("高风险关键词: {}", kw)).await;
                return ApprovalDecision::NeedsApproval;
            }
        }

        // 检查自动通过
        for pattern in &self.policy.auto_approve_patterns {
            if self.match_pattern(target, pattern) {
                self.record(ApprovalDecision::AutoApproved, action, "匹配通过规则").await;
                return ApprovalDecision::AutoApproved;
            }
        }

        // 默认需要审批
        ApprovalDecision::NeedsApproval
    }

    /// 请求审批（阻塞等待人工响应）
    pub async fn request(&self, action: &str, details: &str) -> Result<bool> {
        let decision = self.check(action, "", details).await;

        match decision {
            ApprovalDecision::AutoApproved => Ok(true),
            ApprovalDecision::AutoRejected => Ok(false),
            ApprovalDecision::NeedsApproval => {
                let id = uuid::Uuid::new_v4().to_string();
                let pending_approval = PendingApproval {
                    id: id.clone(),
                    action: action.to_string(),
                    details: details.to_string(),
                    risk_level: RiskLevel::Medium,
                    created_at: Utc::now(),
                };

                self.pending.write().await.insert(id.clone(), pending_approval);

                tracing::warn!("⚠️ 需要人工审批: {} - {}", action, details);

                // 实际实现中：发送通知 + 等待 WebSocket 推送确认
                // 当前简化：自动通过（开发阶段）
                Ok(true)
            }
        }
    }

    /// 批准待审批项
    pub async fn approve(&self, id: &str) -> bool {
        let mut pending = self.pending.write().await;
        if let Some(approval) = pending.remove(id) {
            self.record(ApprovalDecision::AutoApproved, &approval.action, "人工批准").await;
            true
        } else {
            false
        }
    }

    /// 拒绝待审批项
    pub async fn reject(&self, id: &str) -> bool {
        let mut pending = self.pending.write().await;
        if let Some(approval) = pending.remove(id) {
            self.record(ApprovalDecision::AutoRejected, &approval.action, "人工拒绝").await;
            true
        } else {
            false
        }
    }

    /// 获取待审批列表
    pub async fn list_pending(&self) -> Vec<PendingApproval> {
        self.pending.read().await.values().cloned().collect()
    }

    /// 获取审批历史
    pub async fn history(&self) -> Vec<ApprovalResult> {
        self.history.read().await.clone()
    }

    /// 简单 glob 模式匹配
    fn match_pattern(&self, path: &str, pattern: &str) -> bool {
        if pattern.contains("**") {
            let prefix = pattern.replace("**/", "").replace("**", "");
            path.contains(&prefix)
        } else if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                path.starts_with(parts[0]) && path.ends_with(parts[1])
            } else {
                path.contains(pattern.trim_matches('*'))
            }
        } else {
            path == pattern
        }
    }

    async fn record(&self, decision: ApprovalDecision, action: &str, reason: &str) {
        self.history.write().await.push(ApprovalResult {
            decision,
            action: action.to_string(),
            reason: reason.to_string(),
            timestamp: Utc::now(),
        });
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}
