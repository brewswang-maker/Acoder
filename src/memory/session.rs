//! Session Memory — 会话级记忆，存储在 SQLite

use super::MemoryItem;
use anyhow::Result;
use std::path::PathBuf;

pub struct SessionMemory {
    db: rusqlite::Connection,
}

impl SessionMemory {
    pub async fn new(path: PathBuf) -> Result<Self> {
        let conn = rusqlite::Connection::open(&path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                tags TEXT,
                created_at TEXT,
                accessed_at TEXT,
                access_count INTEGER DEFAULT 0,
                importance REAL DEFAULT 0.5
            )",
            [],
        )?;
        Ok(Self { db: conn })
    }

    pub async fn store(&self, item: &MemoryItem) -> Result<()> {
        let tags = serde_json::to_string(&item.tags)?;
        self.db.execute(
            "INSERT OR REPLACE INTO sessions (id, content, tags, created_at, accessed_at, access_count, importance)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                item.id, item.content, tags,
                item.created_at.to_rfc3339(), item.accessed_at.to_rfc3339(),
                item.access_count, item.importance,
            ],
        )?;
        Ok(())
    }

    pub async fn retrieve(&self, _query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        let mut stmt = self.db.prepare(
            "SELECT id, content, tags, created_at, accessed_at, access_count, importance
             FROM sessions ORDER BY accessed_at DESC LIMIT ?1"
        )?;
        let items = stmt.query_map(rusqlite::params![limit], |row| {
            let tags: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
            Ok(MemoryItem {
                id: row.get(0)?,
                content: row.get(1)?,
                memory_type: super::MemoryType::Session,
                tags,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                    .map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
                accessed_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
                access_count: row.get(5)?,
                importance: row.get(6)?,
                embedding_id: None,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }
}
