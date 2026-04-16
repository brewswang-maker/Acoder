//! 嵌入向量支持 — 向量检索的抽象层
//!
//! 当前使用 MockEmbeddingProvider（基于哈希的模拟嵌入），
//! 后续替换为真实向量服务（OpenAI embeddings / 本地 SentenceTransformers）。

use anyhow::Result;

/// 嵌入向量维度（标准维度，Mock 使用 64 维，真实模型通常 768/1536）
pub const EMBEDDING_DIM: usize = 64;

/// 嵌入提供者 trait
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// 将单条文本转换为向量
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// 批量将文本转换为向量
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // 默认实现：逐条调用
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// 向量维度
    fn dimension(&self) -> usize;
}

/// 模拟嵌入提供者 — 基于哈希生成确定性伪向量（开发/测试用）
///
/// 使用 FNV 哈希确保同一文本始终生成相同的向量。
/// 不具备语义相似性，仅用于开发和测试。
#[derive(Debug, Clone)]
pub struct MockEmbeddingProvider {
    dim: usize,
}

impl Default for MockEmbeddingProvider {
    fn default() -> Self {
        Self::new(EMBEDDING_DIM)
    }
}

impl MockEmbeddingProvider {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// 基于文本哈希生成确定性向量
    fn hash_to_vector(&self, text: &str) -> Vec<f32> {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut vec = Vec::with_capacity(self.dim);
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();

        // 基于种子生成伪随机向量，然后归一化
        let mut state = seed;
        for _ in 0..self.dim {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let raw = ((state >> 33) as i64 % 10000) as f32 / 10000.0;
            vec.push(raw);
        }

        // L2 归一化
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vec.iter_mut() {
                *v /= norm;
            }
        }

        vec
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.hash_to_vector(text))
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

/// 余弦相似度计算
///
/// 返回值范围 [-1.0, 1.0]，1.0 表示完全相同方向。
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

/// 批量余弦相似度 — 计算查询向量与多个候选向量的相似度
pub fn batch_cosine_similarity(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates.iter()
        .map(|c| cosine_similarity(query, c))
        .collect()
}

/// 根据余弦相似度找出 top-k 候选
pub fn top_k_by_similarity(
    query: &[f32],
    candidates: &[Vec<f32>],
    k: usize,
) -> Vec<(usize, f32)> {
    let mut scored: Vec<(usize, f32)> = candidates.iter()
        .enumerate()
        .map(|(i, c)| (i, cosine_similarity(query, c)))
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_embedding_deterministic() {
        let provider = MockEmbeddingProvider::new(64);
        let v1 = provider.embed("hello world").await.unwrap();
        let v2 = provider.embed("hello world").await.unwrap();
        assert_eq!(v1, v2);
    }

    #[tokio::test]
    async fn test_mock_embedding_different() {
        let provider = MockEmbeddingProvider::new(64);
        let v1 = provider.embed("hello").await.unwrap();
        let v2 = provider.embed("world").await.unwrap();
        assert_ne!(v1, v2);
    }

    #[tokio::test]
    async fn test_mock_embedding_normalized() {
        let provider = MockEmbeddingProvider::new(64);
        let v = provider.embed("test").await.unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_batch_embedding() {
        let provider = MockEmbeddingProvider::new(32);
        let texts = vec!["hello", "world", "foo"];
        let results = provider.embed_batch(&texts).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_top_k() {
        let query = vec![1.0, 0.0];
        let candidates = vec![
            vec![1.0, 0.0],  // sim = 1.0
            vec![0.0, 1.0],  // sim = 0.0
            vec![0.9, 0.1],  // sim ≈ 0.9
            vec![0.5, 0.5],  // sim ≈ 0.707
        ];
        let top = top_k_by_similarity(&query, &candidates, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, 0); // 最相似的是第一个
        assert_eq!(top[1].0, 2); // 第二相似的是第三个
    }
}
