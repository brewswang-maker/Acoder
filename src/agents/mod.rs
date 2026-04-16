//! Agent 域 — 多智能体协作系统
//!
//! - Commander: 指挥官 Agent，协调所有专家
//! - Expert: 专家 Agent 基类
//! - 200+ 专家实现

pub mod commander;
pub mod expert;
pub mod collaboration;

pub use commander::Commander;
pub use expert::{Expert, ExpertType, ExpertRegistry};
pub use collaboration::CollaborationProtocol;
