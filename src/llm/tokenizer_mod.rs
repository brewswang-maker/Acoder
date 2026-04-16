//! Token 计数器 — 估算 LLM 请求的 Token 消耗
//!
//! 使用 BPE 近似估算（不依赖 tiktoken）：
//! - 中文：约 1.5 字符 / token
//! - 英文：约 4 字符 / token
//! - 代码：约 3.5 字符 / token

use serde::{Deserialize, Serialize};

/// Token 计数结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCount {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
}

/// 估算文本的 Token 数
pub fn estimate_tokens(text: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_chinese = false;

    for ch in text.chars() {
        if is_cjk(ch) {
            if !in_chinese {
                in_chinese = true;
            }
            // 中文约 1.5 字符/token
            tokens += 1;
        } else if ch.is_ascii_whitespace() {
            // 空白字符
            tokens += 1;
        } else if ch.is_ascii() {
            // ASCII（英文/代码）
            if in_chinese {
                in_chinese = false;
            }
            // 每 4 个 ASCII 字符约 1 token
        } else {
            // 其他 Unicode
            tokens += 1;
        }
    }

    // ASCII 字符按 4 字符/token 估算
    let ascii_count = text.chars().filter(|c| c.is_ascii() && !c.is_ascii_whitespace()).count();
    let ascii_tokens = (ascii_count + 3) / 4;
    let non_ascii_tokens = text.chars().filter(|c| !c.is_ascii() || c.is_ascii_whitespace()).count();

    ascii_tokens + non_ascii_tokens
}

/// 判断是否为 CJK 字符
fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}' |   // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |   // CJK Unified Ideographs Extension A
        '\u{20000}'..='\u{2A6DF}' | // CJK Unified Ideographs Extension B
        '\u{2A700}'..='\u{2B73F}' | // CJK Unified Ideographs Extension C
        '\u{2B740}'..='\u{2B81F}' | // CJK Unified Ideographs Extension D
        '\u{F900}'..='\u{FAFF}' |   // CJK Compatibility Ideographs
        '\u{3000}'..='\u{303F}' |   // CJK Symbols and Punctuation
        '\u{3040}'..='\u{309F}' |   // Hiragana
        '\u{30A0}'..='\u{30FF}' |   // Katakana
        '\u{AC00}'..='\u{D7AF}'     // Hangul Syllables
    )
}

/// 估算 LLM 请求成本
pub fn estimate_cost(model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
    let (input_price, output_price) = model_pricing(model);
    (input_tokens as f64 / 1000.0) * input_price + (output_tokens as f64 / 1000.0) * output_price
}

/// 获取模型定价（USD per 1K tokens）
fn model_pricing(model: &str) -> (f64, f64) {
    match model {
        "gpt-4o" => (0.005, 0.015),
        "gpt-4o-mini" => (0.00015, 0.0006),
        "o3" => (0.003, 0.012),
        "o4-mini" => (0.0011, 0.0044),
        "deepseek-chat" | "deepseek-v3" => (0.0001, 0.0001),
        "deepseek-reasoner" | "deepseek-r1" => (0.0004, 0.0016),
        "qwen-plus" => (0.0002, 0.0006),
        "qwen-max" => (0.0004, 0.0012),
        "qwen-coder-plus" => (0.0002, 0.0006),
        "glm-4-plus" => (0.0001, 0.0001),
        "glm-4-flash" => (0.00001, 0.00001),
        "minimax-text-01" => (0.0001, 0.0001),
        _ => (0.001, 0.003), // 默认中等定价
    }
}
