//! 上下文检索器 — 从代码库中检索与任务相关的上下文
//!
//! 检索策略：
//! - 关键词匹配：文件名 + 内容关键词
//! - 调用链追踪：从入口函数追踪依赖
//! - 语义检索：向量相似度（长期目标）
//!
//! 参考 FastCode Scouting-First 策略：
//! 1. 先用轻量级扫描确定候选文件
//! 2. 再深度读取关键文件内容

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// 检索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    /// 检索到的文件
    pub files: Vec<FileContext>,
    /// 检索总 Token 数
    pub total_tokens: usize,
    /// 检索耗时（ms）
    pub duration_ms: u64,
    /// 使用的检索策略
    pub strategy: RetrievalStrategy,
}

/// 单个文件的上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    pub path: String,
    pub relevance_score: f64,
    pub content: String,
    pub tokens: usize,
    pub reason: String,
}

/// 检索策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalStrategy {
    /// 关键词匹配
    Keyword,
    /// 调用链追踪
    CallChain,
    /// 混合策略
    Hybrid,
}

/// 上下文检索器
pub struct ContextRetriever {
    /// 项目根目录
    project_root: PathBuf,
    /// 最大检索 Token 数
    max_tokens: usize,
}

impl ContextRetriever {
    pub fn new(project_root: impl Into<PathBuf>, max_tokens: usize) -> Self {
        Self {
            project_root: project_root.into(),
            max_tokens,
        }
    }

    /// 根据任务描述检索相关上下文
    pub async fn retrieve(&self, task: &str) -> RetrievalResult {
        let start = std::time::Instant::now();

        // 第一步：关键词扫描（Scouting-First）
        let candidates = self.scout_files(task).await;

        // 第二步：深度读取
        let mut files = Vec::new();
        let mut total_tokens = 0;

        for candidate in candidates {
            if total_tokens >= self.max_tokens {
                break;
            }

            let content = self.read_file_content(&candidate.path).await;
            let tokens = crate::llm::tokenizer::estimate_tokens(&content);

            if total_tokens + tokens <= self.max_tokens {
                total_tokens += tokens;
                files.push(candidate.with_content(content, tokens));
            } else {
                // 部分读取（截断到剩余预算）
                let remaining = self.max_tokens - total_tokens;
                let char_budget = remaining * 3;
                if char_budget < content.len() {
                    let partial = format!("{}...[截断]", &content[..char_budget]);
                    let partial_tokens = crate::llm::tokenizer::estimate_tokens(&partial);
                    files.push(candidate.with_content(partial, partial_tokens));
                }
                break;
            }
        }

        RetrievalResult {
            total_tokens,
            duration_ms: start.elapsed().as_millis() as u64,
            strategy: RetrievalStrategy::Hybrid,
            files,
        }
    }

    /// Scouting-First：快速扫描文件，找出候选文件
    async fn scout_files(&self, task: &str) -> Vec<FileContext> {
        let mut candidates = Vec::new();
        let task_lower = task.to_lowercase();

        // 提取任务中的关键词
        let keywords = self.extract_keywords(&task_lower);

        // 遍历项目文件
        if let Ok(entries) = self.walk_project_files() {
            for path in entries {
                let file_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                let mut score = 0.0f64;

                // 文件名匹配
                for kw in &keywords {
                    if file_name.contains(kw) {
                        score += 0.3;
                    }
                }

                // 路径匹配
                let path_str = path.to_string_lossy().to_lowercase();
                for kw in &keywords {
                    if path_str.contains(kw) {
                        score += 0.2;
                    }
                }

                // 特殊文件加权
                if file_name == "main.rs" || file_name == "mod.rs" || file_name == "lib.rs" {
                    score += 0.15;
                }

                if score > 0.0 {
                    candidates.push(FileContext {
                        path: path.to_string_lossy().to_string(),
                        relevance_score: score,
                        content: String::new(),
                        tokens: 0,
                        reason: format!("关键词匹配 (score: {:.2})", score),
                    });
                }
            }
        }

        // 按相关度排序
        candidates.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        candidates
    }

    /// 从任务描述中提取关键词
    fn extract_keywords(&self, task: &str) -> Vec<String> {
        // 简单分词：按空格和标点分割
        let stop_words = ["的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都",
            "一", "一个", "上", "也", "很", "到", "说", "要", "去", "你", "会", "着",
            "没有", "看", "好", "自己", "这", "the", "a", "an", "is", "are", "was",
            "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
            "will", "would", "could", "should", "may", "might", "shall", "can",
            "to", "of", "in", "for", "on", "with", "at", "by", "from", "as"];

        task.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .filter(|w| !w.is_empty() && w.len() > 1)
            .filter(|w| !stop_words.contains(&w.to_lowercase().as_str()))
            .map(|w| w.to_lowercase())
            .collect()
    }

    /// 遍历项目文件
    fn walk_project_files(&self) -> Result<Vec<PathBuf>, std::io::Error> {
        let mut files = Vec::new();
        self.walk_dir(&self.project_root, &mut files, 0)?;
        Ok(files)
    }

    fn walk_dir(&self, dir: &Path, files: &mut Vec<PathBuf>, depth: usize) -> Result<(), std::io::Error> {
        if depth > 5 { return Ok(()); } // 限制深度

        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // 跳过隐藏目录和常见排除目录
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "target" || name == "node_modules"
                    || name == "dist" || name == "build" || name == "__pycache__"
                    || name == ".git" || name == "vendor"
                {
                    continue;
                }
            }

            if path.is_dir() {
                self.walk_dir(&path, files, depth + 1)?;
            } else if path.is_file() {
                // 只包含源代码文件
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext, "rs" | "ts" | "js" | "py" | "go" | "java" | "c" | "cpp"
                        | "h" | "toml" | "yaml" | "yml" | "json" | "md" | "sql" | "html" | "css" | "vue")
                    {
                        files.push(path);
                    }
                }
            }
        }

        Ok(())
    }

    /// 读取文件内容
    async fn read_file_content(&self, path: &str) -> String {
        tokio::fs::read_to_string(path).await.unwrap_or_default()
    }
}

impl FileContext {
    fn with_content(mut self, content: String, tokens: usize) -> Self {
        self.content = content;
        self.tokens = tokens;
        self
    }
}
