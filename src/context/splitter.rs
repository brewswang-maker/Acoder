//! 上下文分割器 — 将长文件分割为适合 LLM 窗口的块
//!
//! 分割策略：
//! - 代码文件：按函数/结构体边界分割
//! - Markdown：按标题分割
//! - 纯文本：按段落 + Token 预算分割
//! - 配置文件：整体保留（通常不大）

use serde::{Deserialize, Serialize};

/// 分割后的块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// 块内容
    pub content: String,
    /// 起始行号
    pub start_line: usize,
    /// 结束行号
    pub end_line: usize,
    /// Token 数
    pub tokens: usize,
    /// 块类型
    pub chunk_type: ChunkType,
}

/// 块类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    /// 函数
    Function,
    /// 结构体/类型定义
    TypeDefinition,
    /// 模块声明
    Module,
    /// 注释块
    Comment,
    /// 段落
    Paragraph,
    /// 其他
    Other,
}

/// 上下文分割器
pub struct ContextSplitter {
    /// 每块最大 Token 数
    max_chunk_tokens: usize,
    /// 块间重叠 Token 数
    overlap_tokens: usize,
}

impl ContextSplitter {
    pub fn new(max_chunk_tokens: usize) -> Self {
        Self {
            max_chunk_tokens,
            overlap_tokens: 100, // 默认重叠 100 tokens
        }
    }

    /// 分割文本
    pub fn split(&self, content: &str, file_type: FileType) -> Vec<Chunk> {
        match file_type {
            FileType::Code => self.split_code(content),
            FileType::Markdown => self.split_markdown(content),
            FileType::PlainText => self.split_by_tokens(content),
            FileType::Config => {
                // 配置文件整体保留
                let tokens = crate::llm::tokenizer::estimate_tokens(content);
                vec![Chunk {
                    content: content.to_string(),
                    start_line: 1,
                    end_line: content.lines().count(),
                    tokens,
                    chunk_type: ChunkType::Other,
                }]
            }
        }
    }

    /// 按 Token 预算分割
    fn split_by_tokens(&self, content: &str) -> Vec<Chunk> {
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let mut current_lines = Vec::new();
        let mut current_tokens = 0;
        let mut start_line = 1;

        for (i, line) in lines.iter().enumerate() {
            let line_tokens = crate::llm::tokenizer::estimate_tokens(line);

            if current_tokens + line_tokens > self.max_chunk_tokens && !current_lines.is_empty() {
                // 保存当前块
                chunks.push(Chunk {
                    content: current_lines.join("\n"),
                    start_line,
                    end_line: start_line + current_lines.len() - 1,
                    tokens: current_tokens,
                    chunk_type: ChunkType::Paragraph,
                });

                // 重叠：保留最后几行
                let overlap_lines = self.find_overlap_lines(&current_lines);
                current_lines = overlap_lines;
                current_tokens = crate::llm::tokenizer::estimate_tokens(&current_lines.join("\n"));
                start_line = i + 1 - current_lines.len() + 1;
            }

            current_lines.push(line.to_string());
            current_tokens += line_tokens;
        }

        // 最后一块
        if !current_lines.is_empty() {
            chunks.push(Chunk {
                content: current_lines.join("\n"),
                start_line,
                end_line: start_line + current_lines.len() - 1,
                tokens: current_tokens,
                chunk_type: ChunkType::Paragraph,
            });
        }

        chunks
    }

    /// 代码分割：按函数/结构体边界
    fn split_code(&self, content: &str) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut current_chunk = Vec::new();
        let mut current_tokens = 0;
        let mut start_line = 1;
        let mut brace_depth: i32 = 0;
        let mut in_item = false;
        let mut item_type = ChunkType::Other;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let line_tokens = crate::llm::tokenizer::estimate_tokens(line);

            // 检测新项目开始
            if !in_item {
                if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub async fn ") || trimmed.starts_with("async fn ")
                {
                    in_item = true;
                    item_type = ChunkType::Function;
                    brace_depth = 0;
                } else if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ")
                    || trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ")
                {
                    in_item = true;
                    item_type = ChunkType::TypeDefinition;
                    brace_depth = 0;
                } else if trimmed.starts_with("mod ") || trimmed.starts_with("use ") {
                    item_type = ChunkType::Module;
                }
            }

            // 跟踪括号深度
            if in_item {
                brace_depth += trimmed.chars().filter(|c| *c == '{').count() as i32;
                brace_depth -= trimmed.chars().filter(|c| *c == '}').count() as i32;
            }

            current_chunk.push(line.to_string());
            current_tokens += line_tokens;

            // 项目结束
            let item_complete = in_item && brace_depth <= 0 && trimmed.contains('}');
            let is_module = matches!(item_type, ChunkType::Module) && trimmed.ends_with(';');

            if item_complete || is_module {
                in_item = false;

                // 如果超过 Token 预算，分割
                if current_tokens >= self.max_chunk_tokens {
                    chunks.push(Chunk {
                        content: current_chunk.join("\n"),
                        start_line,
                        end_line: i + 1,
                        tokens: current_tokens,
                        chunk_type: item_type,
                    });
                    current_chunk = Vec::new();
                    current_tokens = 0;
                    start_line = i + 2;
                }
            }
        }

        // 最后一块
        if !current_chunk.is_empty() {
            chunks.push(Chunk {
                content: current_chunk.join("\n"),
                start_line,
                end_line: start_line + current_chunk.len() - 1,
                tokens: current_tokens,
                chunk_type: item_type,
            });
        }

        chunks
    }

    /// Markdown 分割：按标题
    fn split_markdown(&self, content: &str) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut current_section = Vec::new();
        let mut current_tokens = 0;
        let mut start_line = 1;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // 标题行触发分割
            if (trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### "))
                && !current_section.is_empty()
            {
                chunks.push(Chunk {
                    content: current_section.join("\n"),
                    start_line,
                    end_line: i,
                    tokens: current_tokens,
                    chunk_type: ChunkType::Paragraph,
                });
                current_section = Vec::new();
                current_tokens = 0;
                start_line = i + 1;
            }

            current_section.push(line.to_string());
            current_tokens += crate::llm::tokenizer::estimate_tokens(line);
        }

        if !current_section.is_empty() {
            chunks.push(Chunk {
                content: current_section.join("\n"),
                start_line,
                end_line: start_line + current_section.len() - 1,
                tokens: current_tokens,
                chunk_type: ChunkType::Paragraph,
            });
        }

        chunks
    }

    /// 找出重叠行
    fn find_overlap_lines(&self, lines: &[String]) -> Vec<String> {
        let mut overlap = Vec::new();
        let mut tokens = 0;

        for line in lines.iter().rev() {
            let line_tokens = crate::llm::tokenizer::estimate_tokens(line);
            if tokens + line_tokens > self.overlap_tokens {
                break;
            }
            overlap.push(line.clone());
            tokens += line_tokens;
        }

        overlap.reverse();
        overlap
    }
}

/// 文件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Code,
    Markdown,
    PlainText,
    Config,
}

impl FileType {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" | "ts" | "js" | "py" | "go" | "java" | "c" | "cpp" | "h" | "vue" => FileType::Code,
            "md" | "mdx" => FileType::Markdown,
            "toml" | "yaml" | "yml" | "json" | "ini" | "env" => FileType::Config,
            _ => FileType::PlainText,
        }
    }
}
