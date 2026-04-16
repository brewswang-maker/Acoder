//! LLM 抽象层

pub mod client;
pub mod router;
pub mod prompt;
pub mod tokenizer_mod;
pub mod streaming;
pub mod cache;

pub use client::{
    Client, LlmClientTrait, LlmRequest, LlmResponse, Message, MessageRole,
    ToolCall, LlmTool, TokenUsage, StreamingChunk,
};
pub use router::ModelRouter;
pub use prompt::{PromptTemplate, PromptLibrary};
pub use tokenizer_mod as tokenizer;
pub use streaming::StreamManager;
pub use cache::ResponseCache;
