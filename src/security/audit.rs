//! 审计日志
use std::io::Write;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub user: String,
    pub action: String,
    pub resource: String,
    pub result: AuditResult,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AuditResult { Success, Failed, Blocked, Pending }

pub struct Auditor {
    log_path: PathBuf,
}

impl Auditor {
    pub fn new(log_path: PathBuf) -> Self {
        Self { log_path }
    }

    pub fn log(&self, entry: AuditEntry) -> anyhow::Result<()> {
        let json = serde_json::to_string(&entry)?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?
            .write_all((json + "\n").as_bytes())?;
        Ok(())
    }
}
