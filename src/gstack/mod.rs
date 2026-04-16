//! gstack 模块 — Garry Tan 方法论在 Rust acode 中的落地
//!
//! gstack = AI 开发工作流框架，把 acode 变成虚拟工程团队。

pub mod commands;
pub mod engine;
pub mod exec_log;
pub mod roles;
pub mod template;
pub mod types;
pub mod workspace;

pub use commands::{CommandRegistry, SlashCommand, SlashCommandResult};
pub use engine::ExecutionEngine;
pub use exec_log::{ExecLog, ExecStatus, OutcomeSignal};
pub use roles::Role;
pub use template::{Frontmatter, SkillTemplate, Step, TemplateContext};
pub use types::{StepOutcome, StepResult};
pub use workspace::GstackWorkspace;
