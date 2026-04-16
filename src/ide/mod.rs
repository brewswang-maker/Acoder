//! IDE 集成模块 — LSP Server + 实时代码补全
//!
//! 设计规格: 产品设计文档 v3.0 §11.5, §11.5.1
//!
//! 核心组件：
//! - `lsp_server`: Language Server Protocol 实现（VS Code / Cursor / JetBrains）
//! - `completion`: 实时代码补全引擎（项目上下文 + Skill 模板 + AI 预测）
//! - `diagnostics`: 实时诊断推送
//!
//! 架构：
//!   IDE ←→ LSP Protocol ←→ ACoder LSP Server ←→ ACoder Core
//!                                         ├→ Completion Engine
//!                                         ├→ Diagnostics
//!                                         └→ Session Integration

pub mod lsp_server;
pub mod completion;
pub mod diagnostics;

pub use lsp_server::{AcodeLspServer, LspConfig};
pub use completion::{
    CompletionEngine, CompletionItem, CompletionContext, CompletionSource,
    CompletionRequest, CompletionResponse, InlineCompletionParams,
};
pub use diagnostics::{DiagnosticService, PublishDiagnosticsParams};
