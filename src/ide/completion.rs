//! 实时代码补全引擎 — 多来源补全 + 智能排序
//!
//! 设计规格: §11.5.1
//!
//! 补全来源优先级：
//! 1. Skill 模板匹配（项目内 Skill 库）
//! 2. 项目代码模式（基于文件类型的常见模式）
//! 3. AI 预测（基于上下文的 LLM 补全）
//! 4. 代码片段库（用户/项目级 snippet）

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 补全来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionSource {
    SkillTemplate,
    ProjectPattern,
    AiPrediction,
    Snippet,
}

/// 补全项类型（对应 LSP CompletionItemKind）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Module,
    Variable,
    Field,
    Constant,
    Keyword,
    Snippet,
    File,
    Directory,
}

/// 单个补全项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: String,
    pub source: CompletionSource,
    pub score: f64,
    pub filter_text: Option<String>,
    pub sort_text: Option<String>,
}

/// 补全触发类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Invoked,       // 用户手动触发 (Ctrl+Space)
    Character,     // 输入字符触发
    Incomplete,    // 上次结果不完整，继续补全
}

/// 补全上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionContext {
    pub file_path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub trigger_kind: TriggerKind,
    pub trigger_character: Option<char>,
    /// 光标前的文本片段（用于前缀匹配）
    pub prefix: String,
    /// 光标所在行的完整文本
    pub line_text: String,
    /// 文件语言标识（rust, typescript, python 等）
    pub language: String,
}

/// 补全请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub context: CompletionContext,
    /// 最大返回数量
    pub max_results: usize,
}

/// 补全响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub items: Vec<CompletionItem>,
    pub is_incomplete: bool,
    pub start_column: usize,
    pub end_column: usize,
}

/// 内联补全参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineCompletionParams {
    pub file_path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub trigger_kind: TriggerKind,
    /// 文件内容（按行）
    pub lines: Vec<String>,
}

/// 内联补全结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineCompletionResult {
    pub text: String,
    pub source: CompletionSource,
    pub confidence: f64,
}

/// 代码片段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub prefix: String,
    pub body: String,
    pub description: String,
    pub language: String,
    pub scope: SnippetScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnippetScope {
    Global,
    Project,
    User,
}

/// 补全引擎
pub struct CompletionEngine {
    snippets: Arc<RwLock<HashMap<String, Vec<Snippet>>>>,
    /// debounce 延迟（毫秒）
    debounce_ms: u64,
    /// AI 补全是否启用
    ai_enabled: bool,
    /// 最大 AI 补全 token 数
    max_ai_tokens: usize,
}

impl CompletionEngine {
    pub fn new() -> Self {
        Self {
            snippets: Arc::new(RwLock::new(HashMap::new())),
            debounce_ms: 150,
            ai_enabled: true,
            max_ai_tokens: 256,
        }
    }

    pub fn with_config(debounce_ms: u64, ai_enabled: bool, max_ai_tokens: usize) -> Self {
        Self {
            snippets: Arc::new(RwLock::new(HashMap::new())),
            debounce_ms,
            ai_enabled,
            max_ai_tokens,
        }
    }

    /// 获取 debounce 延迟
    pub fn debounce_ms(&self) -> u64 {
        self.debounce_ms
    }

    /// 注册代码片段
    pub async fn register_snippet(&self, snippet: Snippet) {
        let mut snippets = self.snippets.write().await;
        snippets
            .entry(snippet.language.clone())
            .or_default()
            .push(snippet);
    }

    /// 批量注册代码片段
    pub async fn register_snippets(&self, new_snippets: Vec<Snippet>) {
        let mut snippets = self.snippets.write().await;
        for snippet in new_snippets {
            snippets
                .entry(snippet.language.clone())
                .or_default()
                .push(snippet);
        }
    }

    /// 核心：获取补全建议
    pub async fn complete(&self, request: CompletionRequest) -> CompletionResponse {
        let mut items = Vec::new();

        let prefix = request.context.prefix.to_lowercase();

        if prefix.is_empty() && request.context.trigger_kind == TriggerKind::Character {
            return CompletionResponse {
                items: Vec::new(),
                is_incomplete: false,
                start_column: request.context.column,
                end_column: request.context.column,
            };
        }

        // 来源 1: 代码片段匹配
        let snippet_items = self.match_snippets(&request.context).await;
        items.extend(snippet_items);

        // 来源 2: 项目模式匹配（基于语言）
        let pattern_items = self.match_project_patterns(&request.context);
        items.extend(pattern_items);

        // 来源 3: AI 预测（仅手动触发或高置信度场景）
        if self.ai_enabled && request.context.trigger_kind == TriggerKind::Invoked {
            let ai_items = self.generate_ai_predictions(&request.context).await;
            items.extend(ai_items);
        }

        // 排序：按 score 降序
        items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // 截断
        let is_incomplete = items.len() > request.max_results;
        items.truncate(request.max_results);

        // 计算替换范围
        let start_column = request.context.column.saturating_sub(request.context.prefix.chars().count());

        CompletionResponse {
            items,
            is_incomplete,
            start_column,
            end_column: request.context.column,
        }
    }

    /// 获取内联补全（整行/多行建议）
    pub async fn inline_complete(&self, params: InlineCompletionParams) -> Option<InlineCompletionResult> {
        // 优先使用 AI 补全
        if self.ai_enabled {
            if let Some(result) = self.generate_inline_ai(&params).await {
                return Some(result);
            }
        }

        // fallback: 基于历史模式
        self.match_inline_pattern(&params).await
    }

    // ── 内部方法 ─────────────────────────────────────────────

    /// 代码片段匹配
    async fn match_snippets(&self, ctx: &CompletionContext) -> Vec<CompletionItem> {
        let snippets = self.snippets.read().await;
        let lang_snippets = snippets.get(&ctx.language);

        let mut items = Vec::new();

        if let Some(snips) = lang_snippets {
            for snippet in snips {
                let snippet_prefix = snippet.prefix.to_lowercase();
                if snippet_prefix.starts_with(&ctx.prefix.to_lowercase()) || ctx.prefix.is_empty() {
                    items.push(CompletionItem {
                        label: snippet.prefix.clone(),
                        kind: CompletionKind::Snippet,
                        detail: Some(snippet.description.clone()),
                        documentation: None,
                        insert_text: snippet.body.clone(),
                        source: CompletionSource::Snippet,
                        score: if snippet_prefix == ctx.prefix.to_lowercase() {
                            1.0
                        } else {
                            0.6
                        },
                        filter_text: Some(snippet.prefix.clone()),
                        sort_text: None,
                    });
                }
            }
        }

        items
    }

    /// 项目模式匹配（基于语言常见模式）
    fn match_project_patterns(&self, ctx: &CompletionContext) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let prefix = ctx.prefix.to_lowercase();

        match ctx.language.as_str() {
            "rust" => {
                items.extend(self.rust_patterns(&prefix));
            }
            "typescript" | "javascript" => {
                items.extend(self.ts_patterns(&prefix));
            }
            "python" => {
                items.extend(self.python_patterns(&prefix));
            }
            _ => {}
        }

        items
    }

    fn rust_patterns(&self, prefix: &str) -> Vec<CompletionItem> {
        let patterns = vec![
            ("fn ", CompletionKind::Keyword, "函数定义"),
            ("async fn ", CompletionKind::Keyword, "异步函数定义"),
            ("pub fn ", CompletionKind::Keyword, "公共函数"),
            ("pub async fn ", CompletionKind::Keyword, "公共异步函数"),
            ("impl ", CompletionKind::Keyword, "实现块"),
            ("struct ", CompletionKind::Keyword, "结构体"),
            ("enum ", CompletionKind::Keyword, "枚举"),
            ("trait ", CompletionKind::Keyword, "特征"),
            ("use ", CompletionKind::Keyword, "导入"),
            ("mod ", CompletionKind::Keyword, "模块"),
            ("match ", CompletionKind::Keyword, "匹配"),
            ("if let ", CompletionKind::Keyword, "条件匹配"),
            ("loop ", CompletionKind::Keyword, "循环"),
            ("for ", CompletionKind::Keyword, "迭代循环"),
            ("while ", CompletionKind::Keyword, "条件循环"),
            ("vec![", CompletionKind::Snippet, "Vec 创建"),
            ("HashMap::new()", CompletionKind::Function, "HashMap"),
            ("String::from(", CompletionKind::Function, "String from"),
            ("format!(", CompletionKind::Snippet, "格式化字符串"),
            ("println!(", CompletionKind::Snippet, "打印输出"),
            ("tracing::info!(", CompletionKind::Function, "info 日志"),
            ("tracing::debug!(", CompletionKind::Function, "debug 日志"),
            ("tracing::error!(", CompletionKind::Function, "error 日志"),
            ("tokio::spawn(", CompletionKind::Function, "异步任务"),
            ("Result<", CompletionKind::Struct, "Result 类型"),
            ("Option<", CompletionKind::Struct, "Option 类型"),
            ("Box::new(", CompletionKind::Function, "Box 分配"),
            ("Arc::new(", CompletionKind::Function, "Arc 引用计数"),
            ("#[derive(", CompletionKind::Snippet, "derive 宏"),
            ("#[async_trait]", CompletionKind::Snippet, "async trait"),
            ("Ok(", CompletionKind::Function, "Ok 返回"),
            ("Err(", CompletionKind::Function, "Err 返回"),
            ("self.", CompletionKind::Snippet, "self 引用"),
            ("Self::", CompletionKind::Snippet, "Self 类型路径"),
        ];

        patterns
            .into_iter()
            .filter(|(p, _, _)| p.to_lowercase().starts_with(prefix) || prefix.is_empty())
            .map(|(label, kind, detail)| {
                let score = if label.to_lowercase().starts_with(prefix) { 0.8 } else { 0.4 };
                CompletionItem {
                    label: label.to_string(),
                    kind,
                    detail: Some(detail.to_string()),
                    documentation: None,
                    insert_text: label.to_string(),
                    source: CompletionSource::ProjectPattern,
                    score,
                    filter_text: None,
                    sort_text: None,
                }
            })
            .collect()
    }

    fn ts_patterns(&self, prefix: &str) -> Vec<CompletionItem> {
        let patterns = vec![
            ("function ", CompletionKind::Keyword, "函数"),
            ("const ", CompletionKind::Keyword, "常量"),
            ("interface ", CompletionKind::Keyword, "接口"),
            ("type ", CompletionKind::Keyword, "类型别名"),
            ("async ", CompletionKind::Keyword, "异步"),
            ("export ", CompletionKind::Keyword, "导出"),
            ("import ", CompletionKind::Keyword, "导入"),
            ("class ", CompletionKind::Keyword, "类"),
            ("console.log(", CompletionKind::Snippet, "日志输出"),
            ("Promise<", CompletionKind::Struct, "Promise"),
        ];

        patterns
            .into_iter()
            .filter(|(p, _, _)| p.to_lowercase().starts_with(prefix))
            .map(|(label, kind, detail)| {
                CompletionItem {
                    label: label.to_string(),
                    kind,
                    detail: Some(detail.to_string()),
                    documentation: None,
                    insert_text: label.to_string(),
                    source: CompletionSource::ProjectPattern,
                    score: 0.7,
                    filter_text: None,
                    sort_text: None,
                }
            })
            .collect()
    }

    fn python_patterns(&self, prefix: &str) -> Vec<CompletionItem> {
        let patterns = vec![
            ("def ", CompletionKind::Keyword, "函数定义"),
            ("async def ", CompletionKind::Keyword, "异步函数"),
            ("class ", CompletionKind::Keyword, "类定义"),
            ("import ", CompletionKind::Keyword, "导入"),
            ("from ", CompletionKind::Keyword, "从模块导入"),
            ("self.", CompletionKind::Snippet, "self 引用"),
            ("print(", CompletionKind::Snippet, "打印"),
            ("if ", CompletionKind::Keyword, "条件"),
            ("for ", CompletionKind::Keyword, "循环"),
            ("while ", CompletionKind::Keyword, "循环"),
            ("return ", CompletionKind::Keyword, "返回"),
            ("yield ", CompletionKind::Keyword, "生成器"),
        ];

        patterns
            .into_iter()
            .filter(|(p, _, _)| p.to_lowercase().starts_with(prefix))
            .map(|(label, kind, detail)| {
                CompletionItem {
                    label: label.to_string(),
                    kind,
                    detail: Some(detail.to_string()),
                    documentation: None,
                    insert_text: label.to_string(),
                    source: CompletionSource::ProjectPattern,
                    score: 0.7,
                    filter_text: None,
                    sort_text: None,
                }
            })
            .collect()
    }

    /// AI 预测补全（骨架，实际调用 LLM）
    async fn generate_ai_predictions(&self, _ctx: &CompletionContext) -> Vec<CompletionItem> {
        // TODO: 接入 LLM 进行上下文感知补全
        // 当前返回空，等待 LLM 集成
        Vec::new()
    }

    /// AI 内联补全（骨架）
    async fn generate_inline_ai(&self, _params: &InlineCompletionParams) -> Option<InlineCompletionResult> {
        // TODO: 接入 LLM 生成整行补全
        None
    }

    /// 基于模式的内联补全
    async fn match_inline_pattern(&self, _params: &InlineCompletionParams) -> Option<InlineCompletionResult> {
        // TODO: 基于文件历史模式生成建议
        None
    }
}

impl Default for CompletionEngine {
    fn default() -> Self {
        Self::new()
    }
}
