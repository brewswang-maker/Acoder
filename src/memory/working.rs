//! Working Memory — 当前任务的工作记忆

use std::collections::VecDeque;

pub struct WorkingMemory {
    /// 当前轮次的上下文
    context: Vec<String>,
    /// 历史决策
    decisions: VecDeque<Decision>,
    /// 工具调用历史
    tool_calls: VecDeque<ToolCallRecord>,
    /// Token 预算
    token_budget: usize,
    /// 当前已用 Token
    used_tokens: usize,
}

struct Decision { description: String, rationale: String }
struct ToolCallRecord { name: String, args: String, result: String }

impl WorkingMemory {
    pub fn new(token_budget: usize) -> Self {
        Self {
            context: Vec::new(),
            decisions: VecDeque::with_capacity(50),
            tool_calls: VecDeque::with_capacity(100),
            token_budget,
            used_tokens: 0,
        }
    }

    pub fn push_context(&mut self, text: String) {
        self.context.push(text);
    }

    pub fn add_decision(&mut self, description: &str, rationale: &str) {
        self.decisions.push_back(Decision {
            description: description.into(),
            rationale: rationale.into(),
        });
        if self.decisions.len() > 50 { self.decisions.pop_front(); }
    }

    pub fn add_tool_call(&mut self, name: &str, args: &str, result: &str) {
        self.tool_calls.push_back(ToolCallRecord {
            name: name.into(), args: args.into(), result: result.into(),
        });
        if self.tool_calls.len() > 100 { self.tool_calls.pop_front(); }
    }

    pub fn recent_decisions(&self) -> Vec<&Decision> {
        self.decisions.iter().rev().take(10).collect()
    }

    pub fn tool_history(&self) -> &VecDeque<ToolCallRecord> {
        &self.tool_calls
    }
}
