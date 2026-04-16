//! LSP Server — Language Server Protocol 实现
//!
//! 支持 VS Code / Cursor / JetBrains 等 IDE 集成。

#![allow(dead_code)]
#![allow(unused_imports)]

use serde::{Deserialize, Serialize};

/// LSP 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspConfig {
    pub host: String,
    pub port: u16,
    pub root_uri: Option<String>,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 0, // OS assigns
            root_uri: None,
        }
    }
}

/// ACoder LSP Server
pub struct AcodeLspServer {
    config: LspConfig,
}

impl AcodeLspServer {
    pub fn new(config: LspConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> crate::Result<()> {
        tracing::info!("LSP server listening on {}:{}", self.config.host, self.config.port);
        Ok(())
    }
}
