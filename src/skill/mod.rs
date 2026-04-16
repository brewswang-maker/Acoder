//! Skill 系统 — 自进化技能体系
//!
//! hermes-agent-rs 自进化能力落地：
//! - L1: Thompson Sampling 模型选择
//! - L2: L2PlanDecision 并行度/检查点
//! - L3: Prompt Shaping + 归因分析
//! - 反馈闭环: OutcomeSignal → 归因 → 改进 → canary → 全量/回滚

pub mod registry;
pub mod evolution;
pub mod conductor;

use anyhow::Result;
use std::path::PathBuf;
use clap::Subcommand;

pub use registry::SkillRegistry;
pub use evolution::EvolutionEngine;
pub use conductor::{ConductorContext, ConductorCheckpoint, ConductorMetadata,
    ConductorCheckpointSummary, SkillManifest, ValidationResult, RollbackResult};

// ── Skill CLI 子命令 ────────────────────────────────────────

#[derive(Subcommand, Debug, Clone)]
pub enum SkillCommands {
    /// 列出所有已安装的 Skill
    #[command(name = "list")]
    List {},
    /// 运行指定 Skill
    #[command(name = "run")]
    Run {
        /// Skill 名称
        name: String,
        /// 运行参数
        #[arg(last = true)]
        params: Vec<String>,
    },
    /// 触发 Skill 进化
    #[command(name = "evolve")]
    Evolve {
        /// Skill 名称
        name: String,
    },
    /// 安装 Skill（带安全审计）
    #[command(name = "install")]
    Install {
        /// Skill 名称或路径
        name: String,
        /// 跳过安全确认（危险！）
        #[arg(long)]
        no_confirm: bool,
    },
    /// 扫描 Skill 安全性
    #[command(name = "scan")]
    Scan {
        /// Skill 路径
        path: String,
        /// 启用行为分析（第 4 层）
        #[arg(long)]
        use_behavioral: bool,
        /// 启用 LLM 语义分析（第 5 层）
        #[arg(long)]
        use_llm: bool,
    },
    /// 发布 Skill（Publish Gate 安全审查）
    #[command(name = "publish")]
    Publish {
        /// Skill 路径
        path: String,
    },
    /// 显示 Skill 安全评分
    #[command(name = "score")]
    Score {
        /// Skill 名称
        name: String,
    },
}

pub struct SkillManager {
    registry: SkillRegistry,
    evolution: Option<EvolutionEngine>,
    scanner: crate::security::SkillScanner,
}

impl SkillManager {
    /// 创建 SkillManager
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let registry = SkillRegistry::new()?;
        let evolution = Some(EvolutionEngine::new(data_dir.clone(), false));
        let scanner = crate::security::SkillScanner::new();

        Ok(Self { registry, evolution, scanner })
    }

    /// 列出所有已安装的 Skill
    pub async fn list_skills(&self) -> Result<Vec<SkillInfo>> {
        self.registry.list().await
    }

    /// 运行指定的 Skill
    pub async fn run_skill(&self, name: &str, _params: Vec<String>) -> Result<()> {
        let skill = self.registry.get(name).await?;
        tracing::info!("运行 Skill: {} v{}", skill.id, skill.version);
        Ok(())
    }

    /// 触发 Skill 进化（手动）
    pub async fn evolve(&self, name: &str) -> Result<()> {
        if let Some(ref engine) = self.evolution {
            engine.evolve_skill(name).await?;
        }
        Ok(())
    }

    /// 扫描 Skill 安全性（Publish Gate / Install Gate 共用）
    ///
    /// 返回 (scan_result, can_proceed)
    /// - Critical 漏洞 → can_proceed = false（必须拒绝）
    /// - High 漏洞 → can_proceed = false（需要人工复核）
    /// - Medium/Low → can_proceed = true
    pub async fn scan_skill(&self, path: &str) -> Result<(ScanResult, bool)> {
        let skill_path = PathBuf::from(path);
        if !skill_path.exists() {
            return Err(anyhow::anyhow!("Skill 路径不存在: {}", path));
        }

        let result = self.scanner.scan(&skill_path).await;
        let can_proceed = matches!(result.risk_level, 
            crate::security::SecurityRiskLevel::Low | crate::security::SecurityRiskLevel::Medium
        );

        Ok((result, can_proceed))
    }

    /// 安装 Skill（带安全审计 + Install Gate）
    ///
    /// 流程：
    /// 1. 扫描 Skill 安全性
    /// 2. CRITICAL → 拒绝安装
    /// 3. HIGH → 需要 --confirm 确认
    /// 4. Medium/Low → 直接安装
    pub async fn install(&mut self, name: &str, force: bool) -> Result<()> {
        // 1. 扫描
        let (result, can_proceed) = self.scan_skill(name).await?;

        // 2. 决策
        match result.risk_level {
            crate::security::SecurityRiskLevel::Critical => {
                println!("🚫 安装被拒绝：发现 CRITICAL 漏洞 {} 个", result.findings.len());
                for f in &result.findings {
                    println!("  [{:?}] {}: {} @ {}", f.severity, f.category.name(), f.name, f.location);
                }
                return Err(anyhow::anyhow!("CRITICAL 安全漏洞，禁止安装"));
            }
            crate::security::SecurityRiskLevel::High => {
                if !force {
                    println!("⚠️  发现 HIGH 风险漏洞 {} 个:", result.findings.len());
                    for f in &result.findings {
                        println!("  - {}: {} @ {}", f.category.name(), f.name, f.location);
                    }
                    println!("\n使用 --no-confirm 强制安装（风险自负）");
                    return Err(anyhow::anyhow!("HIGH 风险，需要 --no-confirm 确认"));
                }
                println!("⚠️  强制安装（--no-confirm）：HIGH 风险由用户承担");
            }
            _ => {
                println!("✅ 安全扫描通过（{:?}），评分: {}/100", result.risk_level, result.score);
            }
        }

        // 3. 实际安装（此处调用 registry）
        self.registry.install(name).await?;
        println!("📦 Skill 已安装: {}", name);
        Ok(())
    }

    /// 发布 Skill（Publish Gate）
    ///
    /// 流程：
    /// 1. 调用 SkillScanner 9 层扫描
    /// 2. CRITICAL → 拒绝发布，打印详细信息
    /// 3. HIGH → 拒绝发布，需先修复
    /// 4. Medium/Low → 允许发布
    ///
    /// 符合设计文档要求：
    /// - CRITICAL 拒绝发布
    /// - HIGH 需人工复核
    pub async fn publish(&self, path: &str) -> Result<PublishResult> {
        let (result, _) = self.scan_skill(path).await?;

        let status = match result.risk_level {
            crate::security::SecurityRiskLevel::Critical => {
                println!("🚫 发布被拒绝：发现 CRITICAL 漏洞");
                PublishStatus::Rejected
            }
            crate::security::SecurityRiskLevel::High => {
                println!("🚫 发布被拒绝：发现 HIGH 漏洞");
                PublishStatus::Rejected
            }
            crate::security::SecurityRiskLevel::Medium => {
                println!("⚠️  发布警告：发现 MEDIUM 漏洞");
                PublishStatus::Warning
            }
            crate::security::SecurityRiskLevel::Low => {
                println!("✅ 发布审查通过");
                PublishStatus::Approved
            }
        };

        Ok(PublishResult {
            path: path.to_string(),
            status,
            score: result.score,
            findings: result.findings,
            recommendations: result.recommendations,
        })
    }

    /// 接收任务结果，触发自进化反馈闭环（L1/L2/L3）
    pub async fn on_outcome(&self, signal: &crate::intelligence::outcome::OutcomeSignal) -> Result<()> {
        if let Some(ref engine) = self.evolution {
            engine.on_outcome(signal).await?;
        }
        Ok(())
    }
}

/// 发布结果
#[derive(Debug)]
pub struct PublishResult {
    pub path: String,
    pub status: PublishStatus,
    pub score: u8,
    pub findings: Vec<crate::security::Finding>,
    pub recommendations: Vec<String>,
}

/// 发布状态
#[derive(Debug, Clone, Copy)]
pub enum PublishStatus {
    Approved,
    Warning,
    Rejected,
}

/// 兼容别名
pub type ScanResult = crate::security::ScanResult;

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub success_rate: f64,
    pub utility_score: f64,
}
