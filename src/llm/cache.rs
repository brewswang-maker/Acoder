//! LLM 响应缓存 — 减少 Token 消耗
//!
//! 缓存策略：
//! - 相同请求 → 直接返回缓存
//! - TTL 过期自动清理

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

/// 缓存统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 { return 0.0; }
        self.hits as f64 / (self.hits + self.misses) as f64
    }
}

/// 缓存条目（基于内容的哈希 key）
struct CacheEntry {
    content_json: String,
    created_at: Instant,
    hit_count: usize,
    ttl: Duration,
}

/// LLM 响应缓存
pub struct ResponseCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    stats: Arc<RwLock<CacheStats>>,
    default_ttl: Duration,
    max_entries: usize,
}

impl ResponseCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
            default_ttl: Duration::from_secs(300),
            max_entries: 1000,
        }
    }

    /// 生成缓存 key（基于 model + messages 内容字符串化）
    fn cache_key(model: &str, messages: &[crate::llm::Message]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        model.hash(&mut hasher);
        for msg in messages {
            // 使用 Display trait 转字符串（MessageRole 实现了 Display）
            let role_str = format!("{}", msg.role);
            role_str.hash(&mut hasher);
            msg.content.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    /// 查询缓存
    pub async fn get(&self, model: &str, messages: &[crate::llm::Message]) -> Option<crate::llm::LlmResponse> {
        let key = Self::cache_key(model, messages);
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(&key) {
            if entry.created_at.elapsed() < entry.ttl {
                entry.hit_count += 1;
                self.stats.write().await.hits += 1;
                // 从 JSON 反序列化
                if let Ok(resp) = serde_json::from_str(&entry.content_json) {
                    return Some(resp);
                }
            } else {
                cache.remove(&key);
                self.stats.write().await.evictions += 1;
            }
        }

        self.stats.write().await.misses += 1;
        None
    }

    /// 写入缓存
    pub async fn set(&self, model: &str, messages: &[crate::llm::Message], response: &crate::llm::LlmResponse) {
        let key = Self::cache_key(model, messages);
        let mut cache = self.cache.write().await;

        // LRU 淘汰
        if cache.len() >= self.max_entries {
            if let Some(evict_key) = cache.iter()
                .min_by_key(|(_, v)| v.hit_count)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&evict_key);
                self.stats.write().await.evictions += 1;
            }
        }

        // 序列化为 JSON 存储
        if let Ok(json) = serde_json::to_string(response) {
            cache.insert(key, CacheEntry {
                content_json: json,
                created_at: Instant::now(),
                hit_count: 0,
                ttl: self.default_ttl,
            });
        }
    }

    /// 清理过期条目
    pub async fn cleanup(&self) {
        let mut cache = self.cache.write().await;
        let before = cache.len();
        cache.retain(|_, entry| entry.created_at.elapsed() < entry.ttl);
        let removed = before - cache.len();
        if removed > 0 {
            self.stats.write().await.evictions += removed;
        }
    }

    /// 获取缓存统计
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let mut stats = self.stats.read().await.clone();
        stats.total_entries = cache.len();
        stats
    }

    /// 清空缓存
    pub async fn clear(&self) {
        self.cache.write().await.clear();
    }
}

impl Default for ResponseCache {
    fn default() -> Self { Self::new() }
}
