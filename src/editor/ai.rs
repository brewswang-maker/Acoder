//! AI 补全器 — 对接 ACoder Engine
//!
//! 提供：
//! - Inline Completion（内联补全）
//! - Hover 解释
//! - AI Chat 对话
//! - 代码修复/重构

use std::path::PathBuf;

/// AI 补全器
pub struct AICompletor {
    /// 当前补全状态
    state: AIState,
    /// 当前建议
    suggestion: Option<AISuggestion>,
    /// API 端点
    api_endpoint: Option<String>,
    /// 模型名称
    model: String,
}

/// AI 状态
#[derive(Debug, Clone)]
enum AIState {
    Idle,
    Completing,
    Explaining,
    Chatting,
}

/// AI 建议
#[derive(Debug, Clone)]
pub struct AISuggestion {
    /// 补全文本
    pub text: String,
    /// 起始位置（字节偏移）
    pub start: usize,
    /// 结束位置（字节偏移）
    pub end: usize,
    /// 置信度（0-1）
    pub confidence: f32,
    /// 补全类型
    pub kind: SuggestionKind,
    /// 完整替换范围
    pub replace_range: Option<std::ops::Range<usize>>,
}

/// 建议类型
#[derive(Debug, Clone, Copy)]
pub enum SuggestionKind {
    Completion,
    Fix,
    Refactor,
    Doc,
    Test,
}

impl AICompletor {
    pub fn new() -> Self {
        Self {
            state: AIState::Idle,
            suggestion: None,
            api_endpoint: None,
            model: "deepseek-chat".to_string(),
        }
    }

    /// 请求内联补全
    pub async fn complete(&mut self, code: &str, cursor: usize) -> Option<AISuggestion> {
        self.state = AIState::Completing;

        let prompt = format!(
            r#"代码补全。根据上下文，续写代码。只输出需要补全的部分，不要解释。

```
{}
⟨CURSOR⟩
```"#,
            &code[..cursor.min(code.len())]
        );

        let result = self.call_llm(&prompt).await;
        self.state = AIState::Idle;

        if let Some(text) = result {
            self.suggestion = Some(AISuggestion {
                text: text.clone(),
                start: cursor,
                end: cursor + text.len(),
                confidence: 0.85,
                kind: SuggestionKind::Completion,
                replace_range: None,
            });
            self.suggestion.clone()
        } else {
            None
        }
    }

    /// 解释代码（hover）
    pub async fn explain(&mut self, code: &str, range: std::ops::Range<usize>) -> String {
        self.state = AIState::Explaining;
        let selected = &code[range.clone()];
        let prompt = format!(
            "解释以下代码（简洁，1-3句话）：\n```\n{}\n```",
            selected
        );
        let result = self.call_llm(&prompt).await.unwrap_or_default();
        self.state = AIState::Idle;
        result
    }

    /// 重构建议
    pub async fn refactor(&mut self, code: &str, range: std::ops::Range<usize>) -> Option<String> {
        let selected = &code[range.clone()];
        let prompt = format!(
            "重构以下代码，使用更现代/简洁的写法。只输出重构后的代码：\n```\n{}\n```",
            selected
        );
        let result = self.call_llm(&prompt).await?;
        if result.trim() != selected.trim() && !result.is_empty() {
            Some(result)
        } else {
            None
        }
    }

    /// 生成测试
    pub async fn gen_test(&mut self, code: &str, filename: &str) -> String {
        let prompt = format!(
            "为以下代码生成测试用例（使用合适的测试框架，只输出测试代码）：\n```{}\n{}\n```",
            filename, code
        );
        self.call_llm(&prompt).await.unwrap_or_default()
    }

    /// 修复错误
    pub async fn fix_error(&mut self, code: &str, error: &str) -> Option<String> {
        let prompt = format!(
            "修复以下代码中的错误。错误信息：{}\n\n代码：\n{}\n\n只输出修复后的代码：",
            error, code
        );
        let result = self.call_llm(&prompt).await?;
        if result.trim() != code.trim() && !result.is_empty() {
            Some(result)
        } else {
            None
        }
    }

    /// 取消当前操作
    pub fn cancel(&mut self) {
        self.state = AIState::Idle;
        self.suggestion = None;
    }

    /// 获取当前建议
    pub fn current_suggestion(&self) -> Option<&AISuggestion> {
        self.suggestion.as_ref()
    }

    /// 是否正在工作
    pub fn is_working(&self) -> bool {
        matches!(self.state, AIState::Completing | AIState::Explaining | AIState::Chatting)
    }

    /// 接受建议（将建议文本插入缓冲区）
    pub fn accept(&mut self, buffer: &mut crate::editor::buffer::Buffer) {
        if let Some(ref suggestion) = self.suggestion {
            if let Some(range) = suggestion.replace_range.clone() {
                buffer.replace(range, &suggestion.text);
            } else {
                buffer.insert_at(buffer.cursor(), &suggestion.text);
            }
        }
        self.suggestion = None;
    }

    /// 调用 LLM
    async fn call_llm(&self, prompt: &str) -> Option<String> {
        // TODO: 实现实际 LLM 调用
        // 通过 HTTP 调用 acode 引擎的 LLM 接口
        // 使用 reqwest::Client 发送请求
        None
    }
}

impl Default for AICompletor {
    fn default() -> Self {
        Self::new()
    }
}

/// AI 对话历史
#[derive(Debug, Clone)]
pub struct AIChatSession {
    /// 对话历史
    messages: Vec<ChatMessage>,
    /// 当前文件上下文
    context_file: Option<PathBuf>,
}

/// 对话消息
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}
