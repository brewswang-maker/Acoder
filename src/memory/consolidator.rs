//! 记忆整合器 — 自动整合短期记忆到长期记忆
//!
//! 参考 EverOS 的记忆衰减和强化机制：
//! - 定期扫描 SessionMemory 中高频/高重要性条目
//! - 整合到 LongTermMemory，衰减旧记忆的 importance
//! - 提供基于关键词的记忆选择器

use super::{MemoryItem, MemoryType, SessionMemory, LongTermMemory};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashSet;

/// 整合报告
#[derive(Debug, Clone, Default)]
pub struct ConsolidationReport {
    /// 迁移到长期记忆的条目数
    pub moved_count: usize,
    /// 衰减的旧记忆条目数
    pub decayed_count: usize,
    /// 整合前会话记忆总数
    pub total_session: usize,
    /// 整合后长期记忆总数
    pub total_longterm: usize,
}

impl ConsolidationReport {
    pub fn summary(&self) -> String {
        format!(
            "整合完成: 迁移 {} 条, 衰减 {} 条 | 会话: {}, 长期: {}",
            self.moved_count, self.decayed_count,
            self.total_session, self.total_longterm,
        )
    }
}

/// 记忆衰减策略
///
/// 公式：importance = base * (1 + ln(access_count + 1)) * exp(-age_days / half_life)
#[derive(Debug, Clone)]
pub struct MemoryDecay {
    /// 半衰期天数（默认 30 天）
    pub half_life_days: f64,
    /// 衰减下限（低于此值不再衰减）
    pub min_importance: f64,
}

impl Default for MemoryDecay {
    fn default() -> Self {
        Self {
            half_life_days: 30.0,
            min_importance: 0.01,
        }
    }
}

impl MemoryDecay {
    /// 根据时间和访问次数计算新 importance
    pub fn decay(&self, item: &MemoryItem) -> f64 {
        let age_days = (Utc::now() - item.accessed_at).num_seconds() as f64 / 86400.0;
        let access_factor = 1.0 + (item.access_count as f64 + 1.0).ln();
        let time_factor = (-age_days / self.half_life_days).exp();
        let new_importance = item.importance * access_factor * time_factor;
        new_importance.max(self.min_importance)
    }

    /// 判断条目是否值得保留
    pub fn should_retain(&self, item: &MemoryItem, threshold: f64) -> bool {
        self.decay(item) >= threshold
    }
}

/// 记忆整合器
///
/// 自动将 SessionMemory 中高价值的条目迁移到 LongTermMemory，
/// 同时对旧记忆执行衰减。
pub struct MemoryConsolidator {
    session: SessionMemory,
    longterm: LongTermMemory,
    decay: MemoryDecay,
    /// 迁移阈值：access_count > 此值才考虑迁移
    access_threshold: u32,
    /// 迁移阈值：importance > 此值才考虑迁移
    importance_threshold: f64,
}

impl MemoryConsolidator {
    pub fn new(session: SessionMemory, longterm: LongTermMemory) -> Self {
        Self {
            session,
            longterm,
            decay: MemoryDecay::default(),
            access_threshold: 3,
            importance_threshold: 0.7,
        }
    }

    /// 设置自定义衰减策略
    pub fn with_decay(mut self, decay: MemoryDecay) -> Self {
        self.decay = decay;
        self
    }

    /// 设置迁移阈值
    pub fn with_thresholds(mut self, access: u32, importance: f64) -> Self {
        self.access_threshold = access;
        self.importance_threshold = importance;
        self
    }

    /// 执行一次整合
    pub async fn consolidate(&self) -> Result<ConsolidationReport> {
        // Step 1: 扫描 SessionMemory
        let all_session = self.session.retrieve("", usize::MAX).await?;
        let total_session = all_session.len();

        // Step 2: 筛选高价值条目
        let candidates: Vec<&MemoryItem> = all_session.iter()
            .filter(|item| {
                item.access_count > self.access_threshold
                    || item.importance > self.importance_threshold
            })
            .collect();

        // Step 3: 迁移到 LongTermMemory
        let mut moved_count = 0;
        let mut moved_ids = HashSet::new();

        for item in &candidates {
            let mut longterm_item = (*item).clone();
            longterm_item.memory_type = MemoryType::LongTerm;
            longterm_item.accessed_at = Utc::now();
            // 合并衰减后的 importance
            longterm_item.importance = self.decay.decay(item).max(item.importance);

            self.longterm.store(&longterm_item).await?;
            moved_ids.insert(item.id.clone());
            moved_count += 1;
        }

        // Step 4: 衰减 LongTermMemory 中的旧记忆
        let all_longterm = self.longterm.retrieve("", usize::MAX).await?;
        let mut decayed_count = 0;

        for item in &all_longterm {
            let new_importance = self.decay.decay(item);
            if new_importance < item.importance {
                let mut updated = item.clone();
                updated.importance = new_importance;
                updated.accessed_at = Utc::now();
                self.longterm.store(&updated).await?;
                decayed_count += 1;
            }
        }

        let total_longterm = all_longterm.len() + moved_count;

        Ok(ConsolidationReport {
            moved_count,
            decayed_count,
            total_session,
            total_longterm,
        })
    }
}

/// 记忆选择器 — 根据查询从候选记忆中选出最相关的条目
pub struct MemorySelector {
    decay: MemoryDecay,
}

impl Default for MemorySelector {
    fn default() -> Self {
        Self {
            decay: MemoryDecay::default(),
        }
    }
}

impl MemorySelector {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从候选记忆中选择与查询最相关的条目
    ///
    /// 使用关键词匹配 + importance 排序
    pub fn select_relevant(
        &self,
        query: &str,
        candidates: &[MemoryItem],
        limit: usize,
    ) -> Vec<MemoryItem> {
        let query_lower = query.to_lowercase();
        let query_keywords: Vec<&str> = query_lower.split_whitespace()
            .filter(|w| w.len() > 1)
            .collect();

        let mut scored: Vec<(f64, &MemoryItem)> = candidates.iter()
            .map(|item| {
                let relevance = self.keyword_score(&query_keywords, item);
                let importance = self.decay.decay(item);
                // 综合：关键词匹配权重 0.6 + importance 权重 0.4
                let score = relevance * 0.6 + importance * 0.4;
                (score, item)
            })
            .filter(|(score, _)| *score > 0.05)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        scored.into_iter().map(|(_, item)| item.clone()).collect()
    }

    /// 关键词匹配评分
    fn keyword_score(&self, keywords: &[&str], item: &MemoryItem) -> f64 {
        if keywords.is_empty() {
            return 0.0;
        }

        let content_lower = item.content.to_lowercase();
        let tags_lower: Vec<String> = item.tags.iter().map(|t| t.to_lowercase()).collect();
        let tags_joined = tags_lower.join(" ");

        let mut matches = 0usize;
        let mut total_weight = 0.0f64;

        for (i, keyword) in keywords.iter().enumerate() {
            // 越靠前的关键词权重越高
            let weight = 1.0 - (i as f64 / (keywords.len() as f64 + 1.0));

            if content_lower.contains(keyword) {
                matches += 1;
                total_weight += weight * 1.0;
            }
            if tags_joined.contains(keyword) {
                matches += 1;
                total_weight += weight * 1.5; // tag 匹配加分
            }
        }

        if matches == 0 {
            return 0.0;
        }

        // 归一化到 0~1
        let max_possible = keywords.len() as f64 * 2.5;
        (total_weight / max_possible).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decay_basic() {
        let decay = MemoryDecay::default();
        let item = MemoryItem::new("test content".into(), MemoryType::Session);
        let score = decay.decay(&item);
        // 新创建的条目，age=0，应该接近原始 importance * (1 + ln(1)) = importance
        assert!(score > 0.0);
    }

    #[test]
    fn test_decay_with_access() {
        let decay = MemoryDecay::default();
        let mut item = MemoryItem::new("important".into(), MemoryType::Session);
        item.access_count = 10;
        item.importance = 0.8;
        let score = decay.decay(&item);
        // 高访问次数应该提升 score
        assert!(score >= 0.8);
    }

    #[test]
    fn test_selector() {
        let selector = MemorySelector::new();
        let items = vec![
            MemoryItem {
                id: "1".into(),
                content: "Rust async programming with tokio".into(),
                memory_type: MemoryType::LongTerm,
                tags: vec!["rust".into(), "async".into()],
                created_at: Utc::now(),
                accessed_at: Utc::now(),
                access_count: 5,
                importance: 0.8,
                embedding_id: None,
            },
            MemoryItem {
                id: "2".into(),
                content: "Python web development".into(),
                memory_type: MemoryType::LongTerm,
                tags: vec!["python".into()],
                created_at: Utc::now(),
                accessed_at: Utc::now(),
                access_count: 2,
                importance: 0.5,
                embedding_id: None,
            },
        ];

        let result = selector.select_relevant("rust async tokio", &items, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "1");
    }
}
