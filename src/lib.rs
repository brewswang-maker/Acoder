//! Acode Library Core
//!
//! 所有模块的公开 API 统一从这里导出

pub mod prelude {
    pub use crate::Error;
    pub use anyhow::Result;
}

pub mod error;
pub mod config;
pub mod utils;

// ── Core Abstraction Layer ────────────────────────────────────
pub mod core;

// ── Intelligence — 自进化引擎 ────────────────────────────────
pub mod intelligence;

// ── Core Pipeline ────────────────────────────────────────────
pub mod llm;
pub mod context;
pub mod planning;
pub mod execution;
pub mod agents;
pub mod memory;
pub mod skill;
pub mod code_understanding;
pub mod codegen;

// ── Cross-cutting ────────────────────────────────────────────
pub mod security;
pub mod observability;

// ── Platform ─────────────────────────────────────────────────
pub mod gateway;
pub mod api;
pub mod session;
pub mod sprint;
pub mod tools;
pub mod scaffold;
pub mod terminal;
pub mod editing;
pub mod editor;
pub mod ui;
pub mod debug;
pub mod webhook;
pub mod hooks;
pub mod merge;
pub mod ide;

#[cfg(feature = "ratatui")]
pub mod tui;

pub use config::Config;
pub use error::{Error, Result};
