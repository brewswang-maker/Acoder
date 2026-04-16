//! Acode 配置管理
//!
//! 支持多层级配置：环境变量 > 用户配置 > 默认值

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Context;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// 应用基本信息
    pub app: AppConfig,

    /// LLM 配置
    pub llm: LlmConfig,

    /// 安全配置
    pub security: SecurityConfig,

    /// 记忆配置
    pub memory: MemoryConfig,

    /// Skill 配置
    pub skill: SkillConfig,

    /// 沙箱配置
    pub sandbox: SandboxConfig,

    /// 观测配置
    pub observability: ObservabilityConfig,

    /// 网关配置
    pub gateway: GatewayConfig,

    /// 插件/MCP 配置
    pub plugins: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    /// 应用名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 数据目录
    pub data_dir: PathBuf,
    /// 工作目录
    pub work_dir: PathBuf,
    /// 默认模型
    pub default_model: String,
    /// 最大并发任务数
    pub max_concurrent_tasks: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("acode");
        let work_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        Self {
            name: "Acode".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            data_dir,
            work_dir,
            default_model: "auto".into(),
            max_concurrent_tasks: 4,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    /// 默认 Provider
    pub default_provider: String,
    /// Provider 列表
    pub providers: HashMap<String, LlmProvider>,
    /// 全局限流
    pub rate_limit: RateLimit,
    /// Token 预算
    pub token_budget: TokenBudget,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmProvider {
    pub name: String,
    pub api_type: LlmApiType,
    /// API Key（优先从环境变量读取）
    pub api_key: Option<String>,
    /// API Base URL
    pub base_url: String,
    /// 默认模型
    pub default_model: String,
    /// 可用模型列表
    pub models: Vec<LlmModel>,
    /// 超时（秒）
    pub timeout_secs: u64,
    /// 最大重试次数
    pub max_retries: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum LlmApiType {
    OpenAi,
    Anthropic,
    OpenAiCompatible, // 通义/Qwen、豆包、DeepSeek 等兼容 OpenAI API
    Vertex,
    Bedrock,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// 最大上下文 tokens
    pub max_context_tokens: usize,
    /// 最大输出 tokens
    pub max_output_tokens: usize,
    /// 输入价格（$/1M tokens）
    pub input_price: f64,
    /// 输出价格（$/1M tokens）
    pub output_price: f64,
    /// 支持的功能
    pub capabilities: Vec<ModelCapability>,
    /// 是否支持流式输出
    pub streaming: bool,
    /// 推荐使用场景
    pub recommended_for: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, strum::Display, strum::AsRefStr)]
pub enum ModelCapability {
    FunctionCalling,
    Vision,
    Streaming,
    JsonMode,
    Reasoning,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimit {
    pub requests_per_minute: u32,
    pub tokens_per_minute: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenBudget {
    /// 单次请求最大 tokens
    pub max_request_tokens: usize,
    /// 上下文压缩阈值（%）
    pub compression_threshold_pct: u8,
    /// 保留系统指令的空间（tokens）
    pub system_reserved_tokens: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();

        // ── OpenAI ────────────────────────────────────────────────
        providers.insert("openai".into(), LlmProvider {
            name: "OpenAI".into(),
            api_type: LlmApiType::OpenAi,
            api_key: std::env::var("OPENAI_API_KEY").ok(),
            base_url: "https://api.openai.com/v1".into(),
            default_model: "gpt-4o".into(),
            models: vec![
                LlmModel {
                    id: "gpt-4o".into(),
                    name: "GPT-4o".into(),
                    provider: "openai".into(),
                    max_context_tokens: 128_000,
                    max_output_tokens: 16_384,
                    input_price: 2.5,
                    output_price: 10.0,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Vision, ModelCapability::Streaming, ModelCapability::JsonMode],
                    streaming: true,
                    recommended_for: vec!["代码生成".into(), "复杂推理".into(), "通用对话".into()],
                },
                LlmModel {
                    id: "gpt-4o-mini".into(),
                    name: "GPT-4o Mini".into(),
                    provider: "openai".into(),
                    max_context_tokens: 128_000,
                    max_output_tokens: 16_384,
                    input_price: 0.15,
                    output_price: 0.6,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode],
                    streaming: true,
                    recommended_for: vec!["简单任务".into(), "成本敏感".into()],
                },
                LlmModel {
                    id: "o3".into(),
                    name: "OpenAI o3".into(),
                    provider: "openai".into(),
                    max_context_tokens: 200_000,
                    max_output_tokens: 100_000,
                    input_price: 10.0,
                    output_price: 40.0,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::JsonMode, ModelCapability::Reasoning],
                    streaming: false,
                    recommended_for: vec!["深度推理".into(), "架构设计".into(), "复杂问题".into()],
                },
                LlmModel {
                    id: "o4-mini".into(),
                    name: "OpenAI o4-mini".into(),
                    provider: "openai".into(),
                    max_context_tokens: 128_000,
                    max_output_tokens: 65_536,
                    input_price: 1.1,
                    output_price: 4.4,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::JsonMode, ModelCapability::Reasoning, ModelCapability::Streaming],
                    streaming: true,
                    recommended_for: vec!["代码任务".into(), "快速推理".into()],
                },
            ],
            timeout_secs: 120,
            max_retries: 3,
        });

        // ── DeepSeek ──────────────────────────────────────────────
        // API: https://api.deepseek.com/v1
        providers.insert("deepseek".into(), LlmProvider {
            name: "DeepSeek".into(),
            api_type: LlmApiType::OpenAiCompatible,
            api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: "deepseek-chat".into(),
            models: vec![
                LlmModel {
                    id: "deepseek-chat".into(),
                    name: "DeepSeek V3".into(),
                    provider: "deepseek".into(),
                    max_context_tokens: 640_000,
                    max_output_tokens: 8_192,
                    input_price: 0.27,
                    output_price: 1.1,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode, ModelCapability::Reasoning],
                    streaming: true,
                    recommended_for: vec!["代码生成".into(), "深度推理".into(), "中文场景".into(), "成本最优".into()],
                },
                LlmModel {
                    id: "deepseek-reasoner".into(),
                    name: "DeepSeek R1".into(),
                    provider: "deepseek".into(),
                    max_context_tokens: 640_000,
                    max_output_tokens: 8_192,
                    input_price: 0.55,
                    output_price: 2.19,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::JsonMode, ModelCapability::Reasoning],
                    streaming: false,
                    recommended_for: vec!["复杂推理".into(), "数学问题".into(), "代码挑战".into()],
                },
            ],
            timeout_secs: 120,
            max_retries: 3,
        });

        // ── 阿里百炼 / Qwen ──────────────────────────────────────
        // API: https://dashscope.aliyuncs.com/compatible-mode/v1
        providers.insert("qwen".into(), LlmProvider {
            name: "阿里百炼 / Qwen".into(),
            api_type: LlmApiType::OpenAiCompatible,
            api_key: std::env::var("DASHSCOPE_API_KEY").ok(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            default_model: "qwen-plus".into(),
            models: vec![
                LlmModel {
                    id: "qwen-plus".into(),
                    name: "通义千问 Plus".into(),
                    provider: "qwen".into(),
                    max_context_tokens: 32_768,
                    max_output_tokens: 8_192,
                    input_price: 0.004,
                    output_price: 0.012,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode],
                    streaming: true,
                    recommended_for: vec!["中文对话".into(), "中文代码".into(), "通用任务".into(), "性价比极高".into()],
                },
                LlmModel {
                    id: "qwen-max".into(),
                    name: "通义千问 Max".into(),
                    provider: "qwen".into(),
                    max_context_tokens: 32_768,
                    max_output_tokens: 8_192,
                    input_price: 0.12,
                    output_price: 0.6,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode, ModelCapability::Reasoning],
                    streaming: true,
                    recommended_for: vec!["高质量中文".into(), "复杂推理".into(), "代码生成".into()],
                },
                LlmModel {
                    id: "qwen-turbo".into(),
                    name: "通义千问 Turbo".into(),
                    provider: "qwen".into(),
                    max_context_tokens: 131_072,
                    max_output_tokens: 8_192,
                    input_price: 0.0015,
                    output_price: 0.0045,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode],
                    streaming: true,
                    recommended_for: vec!["快速响应".into(), "大量调用".into(), "成本极敏感".into()],
                },
                LlmModel {
                    id: "qwen-coder-plus".into(),
                    name: "通义千问 Coder Plus".into(),
                    provider: "qwen".into(),
                    max_context_tokens: 32_768,
                    max_output_tokens: 8_192,
                    input_price: 0.008,
                    output_price: 0.024,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode],
                    streaming: true,
                    recommended_for: vec!["代码生成".into(), "代码修复".into(), "代码审查".into()],
                },
            ],
            timeout_secs: 120,
            max_retries: 3,
        });

        // ── 智谱 GLM ─────────────────────────────────────────────
        // API: https://open.bigmodel.cn/api/paas/v4
        providers.insert("glm".into(), LlmProvider {
            name: "智谱 GLM".into(),
            api_type: LlmApiType::OpenAiCompatible,
            api_key: std::env::var("ZHIPU_API_KEY").ok(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            default_model: "glm-4-flash".into(),
            models: vec![
                LlmModel {
                    id: "glm-4-flash".into(),
                    name: "GLM-4 Flash".into(),
                    provider: "glm".into(),
                    max_context_tokens: 128_000,
                    max_output_tokens: 4_096,
                    input_price: 0.001,
                    output_price: 0.001,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode],
                    streaming: true,
                    recommended_for: vec!["中文对话".into(), "快速任务".into(), "成本极低".into()],
                },
                LlmModel {
                    id: "glm-4-plus".into(),
                    name: "GLM-4 Plus".into(),
                    provider: "glm".into(),
                    max_context_tokens: 128_000,
                    max_output_tokens: 8_192,
                    input_price: 0.1,
                    output_price: 0.1,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode, ModelCapability::Reasoning],
                    streaming: true,
                    recommended_for: vec!["中文推理".into(), "代码生成".into(), "复杂任务".into()],
                },
                LlmModel {
                    id: "glm-z1-flash".into(),
                    name: "GLM-Z1 Flash (推理)".into(),
                    provider: "glm".into(),
                    max_context_tokens: 32_768,
                    max_output_tokens: 4_096,
                    input_price: 0.001,
                    output_price: 0.001,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Reasoning],
                    streaming: false,
                    recommended_for: vec!["深度推理".into(), "中文场景".into(), "成本敏感".into()],
                },
            ],
            timeout_secs: 120,
            max_retries: 3,
        });

        // ── MiniMax ──────────────────────────────────────────────
        // API: https://api.minimax.chat/v1
        providers.insert("minimax".into(), LlmProvider {
            name: "MiniMax".into(),
            api_type: LlmApiType::OpenAiCompatible,
            api_key: std::env::var("MINIMAX_API_KEY").ok(),
            base_url: "https://api.minimax.chat/v1".into(),
            default_model: "MiniMax-Text-01".into(),
            models: vec![
                LlmModel {
                    id: "MiniMax-Text-01".into(),
                    name: "MiniMax Text-01".into(),
                    provider: "minimax".into(),
                    max_context_tokens: 1_000_000,
                    max_output_tokens: 16_384,
                    input_price: 0.099,
                    output_price: 0.99,
                    capabilities: vec![ModelCapability::FunctionCalling, ModelCapability::Streaming, ModelCapability::JsonMode],
                    streaming: true,
                    recommended_for: vec!["超长上下文".into(), "文档处理".into(), "中文场景".into()],
                },
            ],
            timeout_secs: 120,
            max_retries: 3,
        });

        // 通用限流
        let rate_limit = RateLimit {
            requests_per_minute: 60,
            tokens_per_minute: 1_000_000,
        };

        let token_budget = TokenBudget {
            max_request_tokens: 100_000,
            compression_threshold_pct: 80,
            system_reserved_tokens: 4000,
        };

        Self {
            default_provider: std::env::var("ACODE_DEFAULT_PROVIDER")
                .unwrap_or_else(|_| "deepseek".into()),
            providers,
            rate_limit,
            token_budget,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecurityConfig {
    /// 是否启用沙箱
    pub sandbox_enabled: bool,
    /// 是否启用人工审批
    pub approval_enabled: bool,
    /// 高危操作审批阈值
    pub high_risk_approval: bool,
    /// 审计日志
    pub audit_log_enabled: bool,
    /// 审计日志路径
    pub audit_log_path: PathBuf,
    /// 沙箱内存限制（MB）
    pub sandbox_memory_mb: u64,
    /// 沙箱超时（秒）
    pub sandbox_timeout_secs: u64,
    /// 沙箱 CPU 限制（核数）
    pub sandbox_cpu_limit: f64,
    /// 允许执行的命令白名单
    pub allowed_commands: Vec<String>,
    /// 禁止执行的命令
    pub blocked_commands: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            sandbox_enabled: true,
            approval_enabled: true,
            high_risk_approval: true,
            audit_log_enabled: true,
            audit_log_path: PathBuf::from("logs/audit.log"),
            sandbox_memory_mb: 512,
            sandbox_timeout_secs: 30,
            sandbox_cpu_limit: 1.0,
            allowed_commands: vec![
                "git".into(), "cargo".into(), "npm".into(), "pnpm".into(),
                "python3".into(), "node".into(), "go".into(), "rustc".into(),
                "ruff".into(), "rustfmt".into(), "clippy".into(),
            ],
            blocked_commands: vec![
                "rm -rf /".into(), "sudo".into(), "chmod 777".into(),
                "eval".into(), "exec".into(), "bash -c".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryConfig {
    /// 工作记忆容量（tokens）
    pub working_memory_tokens: usize,
    /// 会话记忆保留时间（天）
    pub session_retention_days: u32,
    /// 长期记忆向量维度
    pub embedding_dim: usize,
    /// 长期记忆存储路径
    pub storage_path: PathBuf,
    /// RAG 检索 top_k
    pub rag_top_k: usize,
    /// RAG 检索分数阈值
    pub rag_score_threshold: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            working_memory_tokens: 64_000,
            session_retention_days: 30,
            embedding_dim: 1536,
            storage_path: PathBuf::from("data/memory"),
            rag_top_k: 10,
            rag_score_threshold: 0.7,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillConfig {
    /// Skill 定义路径
    pub skills_dir: PathBuf,
    /// 是否启用自进化
    pub evolution_enabled: bool,
    /// 进化触发阈值
    pub evolution_threshold: f64,
    /// Skill 上线质量门槛
    pub quality_gate_threshold: f64,
    /// 是否需要人工审核
    pub human_review_required: bool,
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            skills_dir: PathBuf::from("skills"),
            evolution_enabled: true,
            evolution_threshold: 0.5,
            quality_gate_threshold: 0.7,
            human_review_required: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub runtime: SandboxRuntime,
    pub memory_mb: u64,
    pub timeout_secs: u64,
    pub cpu_limit: f64,
    pub network_enabled: bool,
    pub filesystem_readonly: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum SandboxRuntime {
    Docker,
    Native, // 直接本地执行（仅安全命令）
    #[serde(rename = "wasm")]
    Wasm,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            runtime: SandboxRuntime::Docker,
            memory_mb: 512,
            timeout_secs: 30,
            cpu_limit: 1.0,
            network_enabled: false,
            filesystem_readonly: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfig {
    pub tracing_enabled: bool,
    pub metrics_enabled: bool,
    pub dashboard_port: Option<u16>,
    pub log_level: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            tracing_enabled: true,
            metrics_enabled: true,
            dashboard_port: Some(9090),
            log_level: "info".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayConfig {
    pub http_port: u16,
    pub ws_port: u16,
    pub cors_enabled: bool,
    pub cors_origins: Vec<String>,
    pub rate_limit_per_minute: u32,
    pub max_request_size_mb: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            http_port: 8080,
            ws_port: 8081,
            cors_enabled: true,
            cors_origins: vec!["*".into()],
            rate_limit_per_minute: 60,
            max_request_size_mb: 10,
        }
    }
}

impl Config {
    /// 从多个来源加载配置
    pub fn load() -> anyhow::Result<Self> {
        let mut config = Self::default();

        // 1. 从配置文件加载（如果有）
        let config_paths = vec![
            std::path::PathBuf::from("acode.toml"),
            std::path::PathBuf::from(".acode.toml"),
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("acode")
                .join("config.toml"),
        ];

        for path in &config_paths {
            if path.exists() {
                tracing::info!("加载配置文件: {}", path.display());
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("读取配置文件失败: {}", path.display()))?;
                let loaded: Config = serde_json::from_str(&content)
                    .or_else(|_| serde_json::from_str(&content))
                    .unwrap_or_else(|_| {
                        // 尝试 YAML
                        serde_yaml::from_str(&content).unwrap_or_else(|_| {
                            panic!("无法解析配置文件: {}", path.display())
                        })
                    });
                config.merge(loaded);
                break;
            }
        }

        // 2. 从环境变量覆盖
        config.load_from_env();

        // 3. 验证配置
        config.validate()?;

        Ok(config)
    }

    fn merge(&mut self, other: Config) {
        // 浅合并：other 有值的字段替换 self
        if other.app.name != "Acode" { self.app = other.app; }
        if !other.llm.default_provider.is_empty() { self.llm = other.llm; }
        if other.security.sandbox_enabled != Self::default().security.sandbox_enabled {
            self.security = other.security;
        }
        self.memory = other.memory;
        self.skill = other.skill;
        self.sandbox = other.sandbox;
        self.observability = other.observability;
        self.gateway = other.gateway;
    }

    fn load_from_env(&mut self) {
        if let Ok(provider) = std::env::var("ACODE_DEFAULT_PROVIDER") {
            self.llm.default_provider = provider;
        }
        // OpenAI
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if let Some(p) = self.llm.providers.get_mut("openai") {
                p.api_key = Some(key);
            }
        }
        // DeepSeek
        if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
            if let Some(p) = self.llm.providers.get_mut("deepseek") {
                p.api_key = Some(key);
            }
        }
        // Qwen / 阿里百炼
        if let Ok(key) = std::env::var("DASHSCOPE_API_KEY") {
            if let Some(p) = self.llm.providers.get_mut("qwen") {
                p.api_key = Some(key);
            }
        }
        // 智谱 GLM
        if let Ok(key) = std::env::var("ZHIPU_API_KEY") {
            if let Some(p) = self.llm.providers.get_mut("glm") {
                p.api_key = Some(key);
            }
        }
        // MiniMax
        if let Ok(key) = std::env::var("MINIMAX_API_KEY") {
            if let Some(p) = self.llm.providers.get_mut("minimax") {
                p.api_key = Some(key);
            }
        }
    }

    fn validate(&self) -> anyhow::Result<()> {
        // 确保至少有一个可用的 LLM Provider
        let has_provider = self.llm.providers.values().any(|p| p.api_key.is_some());
        if !has_provider {
            tracing::warn!("⚠️  未配置任何 LLM API Key，部分功能将不可用");
            tracing::warn!("    请设置 OPENAI_API_KEY 或 ANTHROPIC_API_KEY 环境变量");
        }

        // 确保数据目录存在
        std::fs::create_dir_all(&self.memory.storage_path)
            .with_context(|| format!("创建记忆存储目录失败: {}", self.memory.storage_path.display()))?;

        std::fs::create_dir_all(&self.app.data_dir)
            .with_context(|| format!("创建数据目录失败: {}", self.app.data_dir.display()))?;

        Ok(())
    }

    /// 获取指定模型
    pub fn get_model(&self, model_id: &str) -> Option<&LlmModel> {
        for provider in self.llm.providers.values() {
            if let Some(model) = provider.models.iter().find(|m| m.id == model_id) {
                return Some(model);
            }
        }
        None
    }

    /// 获取可用模型列表
    pub fn available_models(&self) -> Vec<&LlmModel> {
        self.llm.providers
            .values()
            .filter(|p| p.api_key.is_some())
            .flat_map(|p| p.models.iter())
            .collect()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app: AppConfig::default(),
            llm: LlmConfig::default(),
            security: SecurityConfig::default(),
            memory: MemoryConfig::default(),
            skill: SkillConfig::default(),
            sandbox: SandboxConfig::default(),
            observability: ObservabilityConfig::default(),
            gateway: GatewayConfig::default(),
            plugins: HashMap::new(),
        }
    }
}
