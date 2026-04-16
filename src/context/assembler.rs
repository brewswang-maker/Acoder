//! 上下文组装器 — 将多源信息组装为 LLM 可用的 Prompt
//!
//! 组装策略：
//! - 系统提示词（固定前缀）
//! - 项目上下文（文件结构、技术栈）
//! - 代码上下文（相关文件内容）
//! - 对话历史（最近 N 轮）
//! - 工具结果（最新观察）
//! - 指令（当前任务）
//!
//! Token 预算分配：
//! - 系统提示: 10%
//! - 项目上下文: 20%
//! - 代码上下文: 30%
//! - 对话历史: 25%
//! - 工具结果: 10%
//! - 指令: 5%

use serde::{Deserialize, Serialize};

/// 上下文组装结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    /// 组装后的完整 Prompt
    pub prompt: String,
    /// 实际使用的 Token 数
    pub tokens_used: usize,
    /// Token 预算
    pub token_budget: usize,
    /// 各部分 Token 分配
    pub allocation: TokenAllocation,
    /// 被截断的部分
    pub truncated: Vec<TruncatedPart>,
}

/// Token 预算分配
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAllocation {
    pub system_prompt: usize,
    pub project_context: usize,
    pub code_context: usize,
    pub conversation_history: usize,
    pub tool_results: usize,
    pub instruction: usize,
}

/// 被截断的部分
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncatedPart {
    pub part: String,
    pub original_tokens: usize,
    pub truncated_tokens: usize,
}

/// 上下文组装器
pub struct ContextAssembler {
    /// 最大 Token 预算
    max_tokens: usize,
}

impl ContextAssembler {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// 组装上下文
    pub fn assemble(
        &self,
        system_prompt: &str,
        project_context: &str,
        code_context: &str,
        conversation_history: &str,
        tool_results: &str,
        instruction: &str,
    ) -> AssembledContext {
        let allocation = TokenAllocation {
            system_prompt: (self.max_tokens as f64 * 0.10) as usize,
            project_context: (self.max_tokens as f64 * 0.20) as usize,
            code_context: (self.max_tokens as f64 * 0.30) as usize,
            conversation_history: (self.max_tokens as f64 * 0.25) as usize,
            tool_results: (self.max_tokens as f64 * 0.10) as usize,
            instruction: (self.max_tokens as f64 * 0.05) as usize,
        };

        let mut truncated = Vec::new();
        let mut parts = Vec::new();

        // 组装各部分（按优先级，高优先级不可截断）
        parts.push(self.truncate_to(system_prompt, allocation.system_prompt, "system_prompt", &mut truncated));
        parts.push(self.truncate_to(project_context, allocation.project_context, "project_context", &mut truncated));
        parts.push(self.truncate_to(code_context, allocation.code_context, "code_context", &mut truncated));
        parts.push(self.truncate_to(conversation_history, allocation.conversation_history, "conversation_history", &mut truncated));
        parts.push(self.truncate_to(tool_results, allocation.tool_results, "tool_results", &mut truncated));
        parts.push(self.truncate_to(instruction, allocation.instruction, "instruction", &mut truncated));

        let prompt = parts.join("\n\n---\n\n");
        let tokens_used = crate::llm::tokenizer::estimate_tokens(&prompt);

        AssembledContext {
            prompt,
            tokens_used,
            token_budget: self.max_tokens,
            allocation,
            truncated,
        }
    }

    /// 截断文本到指定 Token 预算
    fn truncate_to(
        &self,
        text: &str,
        token_budget: usize,
        part_name: &str,
        truncated: &mut Vec<TruncatedPart>,
    ) -> String {
        let original_tokens = crate::llm::tokenizer::estimate_tokens(text);

        if original_tokens <= token_budget {
            return text.to_string();
        }

        // 按 Token 预算估算字符数（保守估算 3 字符/token）
        let char_budget = token_budget * 3;

        truncated.push(TruncatedPart {
            part: part_name.to_string(),
            original_tokens,
            truncated_tokens: token_budget,
        });

        if char_budget >= text.len() {
            return text.to_string();
        }

        // 截断并添加省略标记
        let mut result = text[..char_budget].to_string();
        result.push_str("\n\n[... 截断，原始大小: {} tokens ...]");
        result
    }
}
