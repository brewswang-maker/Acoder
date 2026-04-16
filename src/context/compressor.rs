//! 上下文压缩器 — 智能压缩长上下文
//!
//! 压缩策略：
//! - 代码文件：只保留签名 + 关键注释，删除实现体
//! - 日志输出：只保留首尾行 + 错误行
//! - 重复内容：去重合并
//! - 无关内容：根据任务相关性评分过滤
//!
//! 目标：在保留关键信息的前提下最大化压缩比

use serde::{Deserialize, Serialize};

/// 压缩结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// 压缩后的文本
    pub compressed: String,
    /// 原始 Token 数
    pub original_tokens: usize,
    /// 压缩后 Token 数
    pub compressed_tokens: usize,
    /// 压缩比
    pub ratio: f64,
    /// 使用的压缩策略
    pub strategies: Vec<CompressionStrategy>,
}

/// 压缩策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionStrategy {
    /// 代码签名提取（删除函数体）
    CodeSignature,
    /// 日志截断（保留首尾 + 错误）
    LogTruncation,
    /// 去重合并
    Deduplication,
    /// 相关性过滤
    RelevanceFilter,
}

/// 上下文压缩器
pub struct ContextCompressor {
    /// 目标压缩比（0.0 - 1.0，1.0 = 不压缩）
    target_ratio: f64,
}

impl ContextCompressor {
    pub fn new(target_ratio: f64) -> Self {
        Self { target_ratio: target_ratio.clamp(0.1, 1.0) }
    }

    /// 压缩上下文
    pub fn compress(&self, content: &str, content_type: ContentType) -> CompressionResult {
        let original_tokens = crate::llm::tokenizer::estimate_tokens(content);
        let target_tokens = (original_tokens as f64 * self.target_ratio) as usize;

        let mut strategies = Vec::new();
        let mut result = content.to_string();

        // 按内容类型选择压缩策略
        match content_type {
            ContentType::Code => {
                result = self.compress_code(&result, target_tokens);
                strategies.push(CompressionStrategy::CodeSignature);
            }
            ContentType::Log => {
                result = self.compress_log(&result, target_tokens);
                strategies.push(CompressionStrategy::LogTruncation);
            }
            ContentType::Mixed => {
                // 先尝试去重
                let before = result.len();
                result = self.deduplicate(&result);
                if result.len() < before {
                    strategies.push(CompressionStrategy::Deduplication);
                }
                // 再压缩代码
                result = self.compress_code(&result, target_tokens);
                strategies.push(CompressionStrategy::CodeSignature);
            }
        }

        let compressed_tokens = crate::llm::tokenizer::estimate_tokens(&result);
        let ratio = if original_tokens > 0 {
            compressed_tokens as f64 / original_tokens as f64
        } else {
            1.0
        };

        CompressionResult {
            compressed: result,
            original_tokens,
            compressed_tokens,
            ratio,
            strategies,
        }
    }

    /// 压缩代码：只保留函数签名和关键注释
    fn compress_code(&self, code: &str, target_tokens: usize) -> String {
        let mut result = Vec::new();
        let mut in_function_body = false;
        let mut brace_depth: i32 = 0;

        for line in code.lines() {
            let trimmed = line.trim();

            // 检测函数/结构体签名
            if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ")
                || trimmed.starts_with("pub async fn ") || trimmed.starts_with("async fn ")
                || trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ")
                || trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ")
                || trimmed.starts_with("impl ")
            {
                in_function_body = false;
                brace_depth = 0;
                result.push(line.to_string());
                // 检查是否在同一行有开括号
                if trimmed.contains('{') && !trimmed.contains('}') {
                    in_function_body = true;
                    brace_depth = 1;
                    result.push("    // ... 省略实现 ...".to_string());
                }
                continue;
            }

            // 在函数体内，跟踪括号深度
            if in_function_body {
                brace_depth += trimmed.chars().filter(|c| *c == '{').count() as i32;
                brace_depth -= trimmed.chars().filter(|c| *c == '}').count() as i32;
                if brace_depth <= 0 {
                    in_function_body = false;
                    result.push("}".to_string());
                }
                continue;
            }

            // 保留注释和空行
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.is_empty() {
                result.push(line.to_string());
                continue;
            }

            // 保留 use 语句
            if trimmed.starts_with("use ") || trimmed.starts_with("mod ") {
                result.push(line.to_string());
                continue;
            }

            // 其他行保留
            result.push(line.to_string());

            // 检查 Token 预算
            let current_tokens = crate::llm::tokenizer::estimate_tokens(&result.join("\n"));
            if current_tokens >= target_tokens {
                result.push("// ... 压缩截断 ...".to_string());
                break;
            }
        }

        result.join("\n")
    }

    /// 压缩日志：保留首尾行和错误行
    fn compress_log(&self, log: &str, target_tokens: usize) -> String {
        let lines: Vec<&str> = log.lines().collect();

        if lines.len() <= 20 {
            return log.to_string();
        }

        let mut result = Vec::new();

        // 保留前 5 行
        for line in lines.iter().take(5) {
            result.push(line.to_string());
        }

        // 保留错误行
        for line in lines.iter().skip(5).take(lines.len() - 10) {
            let lower = line.to_lowercase();
            if lower.contains("error") || lower.contains("错误") || lower.contains("failed") || lower.contains("panic") {
                result.push(line.to_string());
            }
        }

        // 保留最后 5 行
        for line in lines.iter().rev().take(5).rev() {
            result.push(line.to_string());
        }

        // 如果仍然太长，截断
        let joined = result.join("\n");
        let tokens = crate::llm::tokenizer::estimate_tokens(&joined);
        if tokens > target_tokens {
            let char_budget = target_tokens * 3;
            if char_budget < joined.len() {
                return format!("{}...\n[日志压缩: 保留首尾和错误行]", &joined[..char_budget]);
            }
        }

        joined
    }

    /// 去重
    fn deduplicate(&self, text: &str) -> String {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || seen.insert(trimmed.to_string()) {
                result.push(line.to_string());
            }
        }

        result.join("\n")
    }
}

/// 内容类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Code,
    Log,
    Mixed,
}
