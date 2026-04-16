//! 核心类型定义
//!
//! 全局共享类型：
//! - 命令输出
//! - 终端后端 trait
//! - 工具 trait

pub mod traits;

pub use traits::{TerminalBackend, CommandOutput};