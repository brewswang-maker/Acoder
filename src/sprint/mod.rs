//! Sprint 工作流 — 7 阶段完整开发闭环
//!
//! Think → Plan → Build → Review → Test → Ship → Reflect
//!
//! CLI 使用 Phase 枚举（ValueEnum），Engine 实现核心逻辑

pub mod engine;

pub use engine::{SprintEngine, SprintPhase, SprintResult, PhaseResult, PhaseStatus};

use clap::ValueEnum;
use anyhow::Result;

/// Sprint 阶段（CLI 入口）
#[derive(ValueEnum, Debug, Clone, Copy, strum::Display)]
pub enum Phase {
    Think,
    Plan,
    Build,
    Review,
    Test,
    Ship,
    Reflect,
    /// 运行完整 Sprint（所有 7 阶段）
    All,
}

impl Phase {
    pub fn to_sprint_phase(&self) -> Option<SprintPhase> {
        match self {
            Phase::Think   => Some(SprintPhase::Think),
            Phase::Plan    => Some(SprintPhase::Plan),
            Phase::Build   => Some(SprintPhase::Build),
            Phase::Review  => Some(SprintPhase::Review),
            Phase::Test    => Some(SprintPhase::Test),
            Phase::Ship    => Some(SprintPhase::Ship),
            Phase::Reflect => Some(SprintPhase::Reflect),
            Phase::All     => None,
        }
    }
}

/// Sprint CLI 包装器
pub struct SprintRunner {
    workdir: std::path::PathBuf,
    config: crate::Config,
}

impl SprintRunner {
    pub fn new(workdir: std::path::PathBuf, config: crate::Config) -> Self {
        Self { workdir, config }
    }

    /// 运行单个阶段
    pub async fn run_phase(&self, phase: Phase, task: Option<&str>) -> Result<()> {
        if let Some(sp) = phase.to_sprint_phase() {
            let task = task.unwrap_or("未指定任务");
            tracing::info!("Sprint 阶段 {}: {}", sp, task);

            let engine = SprintEngine::new(self.workdir.clone(), self.config.clone()).await?;
            let (output, artifacts, warnings) = engine.run_phase(sp, task).await?;

            println!("\n=== {} ===", sp);
            if !output.is_empty() {
                println!("{}", &output[..output.len().min(500)]);
            }
            if !warnings.is_empty() {
                println!("警告: {:?}", warnings);
            }
            if !artifacts.is_empty() {
                println!("产物: {:?}", artifacts);
            }
            Ok(())
        } else {
            // Phase::All — 运行完整 Sprint
            let task = task.unwrap_or("未指定任务");
            println!("\n🚀 运行完整 Sprint（7 阶段）");
            let engine = SprintEngine::new(self.workdir.clone(), self.config.clone()).await?;
            let result = engine.run_full_sprint(task).await?;
            println!("\n{}", result.summary);
            Ok(())
        }
    }
}
