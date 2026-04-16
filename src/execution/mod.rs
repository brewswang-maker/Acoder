//! 执行引擎（Execution Engine）
//!
//! 行动中心：真正执行代码、调用工具、处理结果
//!
//! 核心组件：
//! - Engine：主执行引擎，整合所有层
//! - ToolRegistry：工具注册表
//! - Sandbox：沙箱执行
//! - Feedback：结构化反馈

pub mod engine;
pub mod tool_registry;
pub mod sandbox;

pub use engine::{Engine, ExecutionResult, Artifact};
pub use tool_registry::ToolRegistry;
pub use sandbox::Sandbox;
