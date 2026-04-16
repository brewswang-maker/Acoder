//! Diagnostics — 实时诊断推送
//!
//! 将编译错误、lint 警告等推送到 IDE。

#![allow(dead_code)]
#![allow(unused_imports)]

use serde::{Deserialize, Serialize};

/// 诊断服务
pub struct DiagnosticService {
    uri: String,
}

impl DiagnosticService {
    pub fn new(uri: String) -> Self {
        Self { uri }
    }

    pub async fn publish(&self, params: PublishDiagnosticsParams) -> crate::Result<()> {
        tracing::info!(
            "Publishing {} diagnostics for {}",
            params.diagnostics.len(),
            params.uri
        );
        Ok(())
    }
}

/// 发布诊断参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub diagnostics: Vec<DiagnosticItem>,
}

/// 诊断项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticItem {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}
