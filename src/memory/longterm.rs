//! Long-term Memory — 长期记忆，跨会话持久化
//!
//! 支持：
//! - SQLite 存储（持久化）
//! - 向量检索（语义相似度）
//! - 关键词匹配（传统检索）
//! - 记忆重要性排序

use super::MemoryItem;
use super::embedding::{EmbeddingProvider, MockEmbeddingProvider, cosine_similarity, top_k_by_similarity};
use anyhow::Result;
use std::path::PathBuf;
use std::collections::HashMap;

/// 长期记忆存储
pub struct LongTermMemory {
    db: rusqlite::Connection,
    /// 嵌入提供者（用于向量检索）
    embedder: Box<dyn EmbeddingProvider>,
    /// 嵌入缓存（内存）
    embedding_cache: HashMap<String, Vec<f32>>,
}

impl LongTermMemory {
    pub async fn new(path: PathBuf) -> Result<Self> {
        let conn = rusqlite::Connection::open(&path)?;
        
        // 创建表
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS longterm (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                tags TEXT,
                created_at TEXT,
                accessed_at TEXT,
                access_count INTEGER DEFAULT 0,
                importance REAL DEFAULT 0.5,
                embedding_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tags ON longterm(tags);
            CREATE INDEX IF NOT EXISTS idx_importance ON longterm(importance DESC);
            CREATE INDEX IF NOT EXISTS idx_accessed ON longterm(accessed_at DESC);
        "#)?;
        
        Ok(Self {
            db: conn,
            embedder: Box::new(MockEmbeddingProvider::default()),
            embedding_cache: HashMap::new(),
        })
    }

    /// 使用自定义嵌入提供者创建
    pub fn with_embedder(mut self, embedder: Box<dyn EmbeddingProvider>) -> Self {
        self.embedder = embedder;
        self
    }

    /// 存储记忆项
    pub async fn store(&self, item: &MemoryItem) -> Result<()> {
        let tags = serde_json::to_string(&item.tags)?;
        self.db.execute(
            "INSERT OR REPLACE INTO longterm 
             (id, content, tags, created_at, accessed_at, access_count, importance, embedding_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                &item.id,
                &item.content,
                &tags,
                item.created_at.to_rfc3339(),
                item.accessed_at.to_rfc3339(),
                item.access_count,
                item.importance,
                item.embedding_id.as_ref().map(|s| s.as_str()),
            ],
        )?;
        Ok(())
    }

    /// 关键词检索（传统方式）
    pub async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        let like = format!("%{}%", query);
        let mut stmt = self.db.prepare(
            "SELECT id, content, tags, created_at, accessed_at, access_count, importance, embedding_id
             FROM longterm 
             WHERE content LIKE ?1 OR tags LIKE ?1
             ORDER BY importance DESC, access_count DESC 
             LIMIT ?2"
        )?;
        
        let items = stmt.query_map(rusqlite::params![like, limit], |row| {
            let tags: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
            let created_str: String = row.get(3)?;
            let accessed_str: String = row.get(4)?;
            
            Ok(MemoryItem {
                id: row.get(0)?,
                content: row.get(1)?,
                memory_type: super::MemoryType::LongTerm,
                tags,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                accessed_at: chrono::DateTime::parse_from_rfc3339(&accessed_str)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                access_count: row.get(5)?,
                importance: row.get(6)?,
                embedding_id: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(items)
    }

    /// 向量检索（语义相似度）
    pub async fn retrieve_semantic(&mut self, query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        // Step 1: 获取所有记忆项
        let all_items = self.get_all().await?;
        if all_items.is_empty() {
            return Ok(Vec::new());
        }

        // Step 2: 计算查询向量
        let query_embedding = self.embedder.embed(query).await?;

        // Step 3: 收集所有记忆的嵌入向量（使用缓存）
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(all_items.len());
        for item in &all_items {
            if let Some(cached) = self.embedding_cache.get(&item.id) {
                embeddings.push(cached.clone());
            } else {
                let emb = self.embedder.embed(&item.content).await?;
                self.embedding_cache.insert(item.id.clone(), emb.clone());
                embeddings.push(emb);
            }
        }

        // Step 4: 计算相似度并排序
        let top_indices = top_k_by_similarity(&query_embedding, &embeddings, limit);

        // Step 5: 返回 top-k 记忆项
        let results: Vec<MemoryItem> = top_indices.into_iter()
            .filter_map(|(idx, sim)| {
                if sim > 0.3 { // 相似度阈值
                    let mut item = all_items.get(idx)?.clone();
                    // 更新访问计数（异步后台更新）
                    self.update_access_count(&item.id).ok();
                    Some(item)
                } else {
                    None
                }
            })
            .collect();

        Ok(results)
    }

    /// 混合检索：关键词 + 向量
    pub async fn retrieve_hybrid(&mut self, query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        // 同时进行关键词和向量检索
        let keyword_results = self.retrieve(query, limit).await?;
        let semantic_results = self.retrieve_semantic(query, limit).await?;

        // 合并去重（优先保留相似度高的）
        let mut seen = std::collections::HashSet::new();
        let mut combined = Vec::new();

        for item in semantic_results.into_iter().chain(keyword_results.into_iter()) {
            if seen.insert(item.id.clone()) {
                combined.push(item);
                if combined.len() >= limit {
                    break;
                }
            }
        }

        Ok(combined)
    }

    /// 获取所有记忆项
    async fn get_all(&self) -> Result<Vec<MemoryItem>> {
        let mut stmt = self.db.prepare(
            "SELECT id, content, tags, created_at, accessed_at, access_count, importance, embedding_id
             FROM longterm ORDER BY importance DESC"
        )?;
        
        let items = stmt.query_map([], |row| {
            let tags: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
            let created_str: String = row.get(3)?;
            let accessed_str: String = row.get(4)?;
            
            Ok(MemoryItem {
                id: row.get(0)?,
                content: row.get(1)?,
                memory_type: super::MemoryType::LongTerm,
                tags,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                accessed_at: chrono::DateTime::parse_from_rfc3339(&accessed_str)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                access_count: row.get(5)?,
                importance: row.get(6)?,
                embedding_id: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(items)
    }

    /// 更新访问计数
    fn update_access_count(&self, id: &str) -> Result<()> {
        self.db.execute(
            "UPDATE longterm SET access_count = access_count + 1, accessed_at = ?1 WHERE id = ?2",
            rusqlite::params![chrono::Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    /// 更新记忆重要性
    pub async fn update_importance(&self, id: &str, importance: f64) -> Result<()> {
        self.db.execute(
            "UPDATE longterm SET importance = ?1 WHERE id = ?2",
            rusqlite::params![importance, id],
        )?;
        Ok(())
    }

    /// 删除记忆
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.db.execute("DELETE FROM longterm WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    /// 清理低重要性记忆（内存优化）
    pub async fn cleanup(&self, keep_top: usize) -> Result<usize> {
        let deleted = self.db.execute(
            "DELETE FROM longterm WHERE id NOT IN (
                SELECT id FROM longterm ORDER BY importance DESC LIMIT ?1
            )",
            rusqlite::params![keep_top],
        )?;
        Ok(deleted)
    }

    /// 统计信息
    pub async fn stats(&self) -> Result<MemoryStats> {
        let count: i64 = self.db.query_row("SELECT COUNT(*) FROM longterm", [], |row| row.get(0))?;
        let avg_importance: f64 = self.db.query_row(
            "SELECT AVG(importance) FROM longterm",
            [], |row| row.get(0)
        ).unwrap_or(0.5);
        let total_access: i64 = self.db.query_row(
            "SELECT SUM(access_count) FROM longterm",
            [], |row| row.get(0)
        ).unwrap_or(0);
        
        Ok(MemoryStats {
            total_items: count as usize,
            avg_importance,
            total_access_count: total_access as usize,
        })
    }
}

/// 记忆统计信息
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_items: usize,
    pub avg_importance: f64,
    pub total_access_count: usize,
}
