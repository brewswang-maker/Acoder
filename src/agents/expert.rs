//! Expert — 专家 Agent 定义（100+ 专家覆盖所有编程领域）
//!
//! 包含 8 大类专家：
//! 1. 语言专家（Rust/Go/Python/TS/Java/C++ 等 20 个）
//! 2. 前端专家（React/Vue/Angular/移动端等 12 个）
//! 3. 后端专家（API/微服务/Serverless 等 15 个）
//! 4. 数据专家（ML/大数据/数据分析等 10 个）
//! 5. DevOps/基础设施专家（K8s/Docker/云等 15 个）
//! 6. 数据库专家（SQL/NoSQL/图数据库等 10 个）
//! 7. 安全/质量专家（审计/测试/性能等 10 个）
//! 8. 领域专家（游戏/嵌入式/区块链等 10 个）

use serde::{Deserialize, Serialize};

/// 专家 Agent 类型（100+）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
pub enum ExpertType {
    // ── 基础角色 ───────────────────────────────────────
    Commander,
    Coder,
    Frontend,
    Backend,
    Security,
    Tester,
    Reviewer,
    Architect,
    DevOps,
    Database,
    Mobile,
    Data,
    API,
    Testing,
    Performance,

    // ── 语言专家 ───────────────────────────────────────
    Rust,
    Go,
    Python,
    TypeScript,
    JavaScript,
    Java,
    Kotlin,
    Swift,
    C,
    Cpp,
    CSharp,
    Ruby,
    PHP,
    Dart,
    R,
    Scala,
    Elixir,
    Clojure,
    Haskell,
    Lua,
    Perl,
    Zig,
    Nim,
    OCaml,
    FSharp,

    // ── 前端专家 ───────────────────────────────────────
    React,
    Vue,
    Angular,
    Svelte,
    NextJS,
    Nuxt,
    SolidJS,
    Qwik,
    ReactNative,
    Flutter,
    Tauri,
    Electron,
    CSS,
    Tailwind,
    WebAssembly,

    // ── 后端专家 ───────────────────────────────────────
    REST,
    GraphQL,
    gRPC,
    Microservice,
    Serverless,
    EventDriven,
    CQRS,
    MessageQueue,
    Redis,
    Nginx,
    Apache,
    Lambda,
    Cloudflare,

    // ── 数据专家 ───────────────────────────────────────
    ML,
    DeepLearning,
    DataEngineering,
    DataAnalysis,
    DataVisualization,
    NLP,
    CV,
    ReinforcementLearning,
    MLOps,
    VectorDB,

    // ── DevOps/基础设施 ────────────────────────────────
    Kubernetes,
    Docker,
    Terraform,
    Ansible,
    AWS,
    GCP,
    Azure,
    CiCd,
    Prometheus,
    Grafana,
    ELK,
    Vault,
    Consul,
    Helm,
    ArgoCD,

    // ── 数据库专家 ─────────────────────────────────────
    PostgreSQL,
    MySQL,
    MongoDB,
    Neo4j,
    Elasticsearch,
    ClickHouse,
    Cassandra,
    DynamoDB,
    SQLite,
    TimescaleDB,

    // ── 安全/质量专家 ──────────────────────────────────
    PenTest,
    AppSec,
    ThreatModeling,
    OWASP,
    LoadTesting,
    ChaosEngineering,
    CodeQuality,
    TechDebt,
    Accessibility,
    Internationalization,

    // ── 领域专家 ───────────────────────────────────────
    GameDev,
    Embedded,
    IoT,
    Blockchain,
    NFT,
    DeFi,
    Web3,
    RPA,
    LowCode,
    NoCode,
// ── 平台/中间件 ────────────────────────────────────
    Kafka,
    RabbitMQ,
    NATS,
    etcd,
    Zookeeper,

    // ── 云服务扩展 ─────────────────────────────────────
    DigitalOcean,
    Vultr,
    Heroku,
    Netlify,
    Vercel,
    CloudflareWorkers,
    Supabase,
    Firebase,
    Appwrite,

    // ── 安全专家扩展 ────────────────────────────────────
    IAM,
    OAuth2,
    OpenID,
    ZeroTrust,
    SOC2,
    GDPR,
    WAF,
    SecretManagement,

    // ── 质量/测试扩展 ──────────────────────────────────
    E2ETesting,
    SnapshotTesting,
    PropertyBasedTesting,
    MutationTesting,
    ContractTesting,
    PerformanceTesting,
    SecurityTesting,

    // ── 数据扩展 ────────────────────────────────────────
    Spark,
    Flink,
    Airflow,
    dbt,
    DuckDB,
    Polars,
    DataPipelines,
    FeatureStore,
    FeatureFlags,

    // ── 新兴技术 ────────────────────────────────────────
    WebGPU,
    WebNN,
    WebRTC,
    WebAssemblyWASI,
    EdgeComputing,
    FederatedLearning,

    // ── 平台工程 ────────────────────────────────────────
    PlatformEngineering,
    InternalDeveloperPortal,
    Backstage,
    DeveloperExperience,
    SRE,
    SiteReliability,

    // ── 架构风格 ────────────────────────────────────────
    HexagonalArchitecture,
    DDD,
    EventSourcing,
    SagaPattern,
    BlueGreen,
    Canary,
    MicroFrontend,

    // ── 协议/标准 ──────────────────────────────────────
    WebAuthn,
    mTLS,
    SPIFFE,
    OpenAPI,
    AsyncAPI,
    GraphQLSubscriptions,

    // ── 观测扩展 ────────────────────────────────────────
    OpenTelemetry,
    Jaeger,
    Datadog,
    Sentry,

    // ── 角色扩展 ────────────────────────────────────────
    SREEngineer,
    MLOpsEngineer,
    DataEngineer,
    DevSecOps,
    PlatformEngineer,
    NetworkEngineer,
    TechLead,
    StaffEngineer,
    PrincipalEngineer,

    // ── 框架/库专家 ──────────────────────────────────────
    Axum,
    ActixWeb,
    Leptos,
    Dioxus,
    NextAuth,
    tRPC,
    Prisma,
    Drizzle,
    shadcnUI,
    SwiftUI,
    JetpackCompose,
    Django,
    NestJS,
    Fiber,
    Echo,
    Spring,
    Micronaut,

    // ── DevOps 扩展 ──────────────────────────────────────
    Packer,
    Pulumi,
    Nomad,
    Buildkite,
    GitHubActions,
    GitLabCI,
    Jenkins,
    ArgoWorkflows,
    Tekton,
    Kustomize,

    // ── 存储扩展 ─────────────────────────────────────────
    S3,
    GCS,
    AzureBlob,
    MinIO,
    Rook,
    Longhorn,

    // ── 身份认证扩展 ─────────────────────────────────────
Auth0,
    Clerk,
    SupabaseAuth,
    KeyCloak,
    AWSIAM,
    AzureAD,

    // ── 消息系统扩展 ──────────────────────────────────────
    KafkaMQ,
    AMQP,
    STOMP,
    MQTT,

    // ── 前端扩展 ─────────────────────────────────────────
    Astro,
    Remix,
    SvelteKit,
    QwikCity,
    Redwood,
    Blitz,
    BlitzJS,

    // ── AI/LLM 专家扩展 ──────────────────────────────────
    LLMEval,
    PromptEngineering,
    RAG,
    FineTuning,
    ModelServing,
    AIInference,
    LangChain,
    LlamaIndex,
    AutoGPT,
    AgentFramework,

    // ── 区块链扩展 ───────────────────────────────────────
    Solidity,
    Vyper,
    RustZK,
    Move,
    ADA,
    CosmosSDK,
    Substrate,

    // ── 游戏开发扩展 ─────────────────────────────────────
    Unreal,
    Godot,
    Unity,
    Bevy,
    ggez,
}

impl ExpertType {
    pub fn all() -> Vec<ExpertType> {
        vec![
            // 基础
            ExpertType::Commander, ExpertType::Coder, ExpertType::Frontend,
            ExpertType::Backend, ExpertType::Security, ExpertType::Tester,
            ExpertType::Reviewer, ExpertType::Architect, ExpertType::DevOps,
            ExpertType::Database, ExpertType::Mobile, ExpertType::Data,
            ExpertType::API, ExpertType::Testing, ExpertType::Performance,
            // 语言
            ExpertType::Rust, ExpertType::Go, ExpertType::Python,
            ExpertType::TypeScript, ExpertType::JavaScript, ExpertType::Java,
            ExpertType::Kotlin, ExpertType::Swift, ExpertType::C, ExpertType::Cpp,
            ExpertType::CSharp, ExpertType::Ruby, ExpertType::PHP, ExpertType::Dart,
            ExpertType::R, ExpertType::Scala, ExpertType::Elixir, ExpertType::Clojure,
            ExpertType::Haskell, ExpertType::Lua, ExpertType::Perl, ExpertType::Zig,
            ExpertType::Nim, ExpertType::OCaml, ExpertType::FSharp,
            // 前端
            ExpertType::React, ExpertType::Vue, ExpertType::Angular,
            ExpertType::Svelte, ExpertType::NextJS, ExpertType::Nuxt,
            ExpertType::SolidJS, ExpertType::Qwik, ExpertType::ReactNative,
            ExpertType::Flutter, ExpertType::Tauri, ExpertType::Electron,
            ExpertType::CSS, ExpertType::Tailwind, ExpertType::WebAssembly,
            // 后端
            ExpertType::REST, ExpertType::GraphQL, ExpertType::gRPC,
            ExpertType::Microservice, ExpertType::Serverless, ExpertType::EventDriven,
            ExpertType::CQRS, ExpertType::MessageQueue, ExpertType::Redis,
            ExpertType::Nginx, ExpertType::Apache, ExpertType::Lambda,
            ExpertType::Cloudflare,
            // 数据
            ExpertType::ML, ExpertType::DeepLearning, ExpertType::DataEngineering,
            ExpertType::DataAnalysis, ExpertType::DataVisualization,
            ExpertType::NLP, ExpertType::CV, ExpertType::ReinforcementLearning,
            ExpertType::MLOps, ExpertType::VectorDB,
            // DevOps
            ExpertType::Kubernetes, ExpertType::Docker, ExpertType::Terraform,
            ExpertType::Ansible, ExpertType::AWS, ExpertType::GCP, ExpertType::Azure,
            ExpertType::CiCd, ExpertType::Prometheus, ExpertType::Grafana,
            ExpertType::ELK, ExpertType::Vault, ExpertType::Consul,
            ExpertType::Helm, ExpertType::ArgoCD,
            // 数据库
            ExpertType::PostgreSQL, ExpertType::MySQL, ExpertType::MongoDB,
            ExpertType::Neo4j, ExpertType::Elasticsearch, ExpertType::ClickHouse,
            ExpertType::Cassandra, ExpertType::DynamoDB, ExpertType::SQLite,
            ExpertType::TimescaleDB,
            // 安全/质量
            ExpertType::PenTest, ExpertType::AppSec, ExpertType::ThreatModeling,
            ExpertType::OWASP, ExpertType::LoadTesting, ExpertType::ChaosEngineering,
            ExpertType::CodeQuality, ExpertType::TechDebt,
            ExpertType::Accessibility, ExpertType::Internationalization,
            // 领域
            ExpertType::GameDev, ExpertType::Embedded, ExpertType::IoT,
            ExpertType::Blockchain, ExpertType::NFT, ExpertType::DeFi,
            ExpertType::Web3, ExpertType::RPA, ExpertType::LowCode, ExpertType::NoCode,
        ]
    }

    /// 专家分组
    pub fn category(&self) -> &'static str {
        match self {
            // 基础
            ExpertType::Commander => "角色",
            ExpertType::Coder => "角色",
            ExpertType::Frontend => "角色",
            ExpertType::Backend => "角色",
            ExpertType::Security => "角色",
            ExpertType::Tester => "角色",
            ExpertType::Reviewer => "角色",
            ExpertType::Architect => "角色",
            ExpertType::DevOps => "角色",
            ExpertType::Database => "角色",
            ExpertType::Mobile => "角色",
            ExpertType::Data => "角色",
            ExpertType::API => "角色",
            ExpertType::Testing => "角色",
            ExpertType::Performance => "角色",
            // 语言
            ExpertType::Rust | ExpertType::Go | ExpertType::Python |
            ExpertType::TypeScript | ExpertType::JavaScript | ExpertType::Java |
            ExpertType::Kotlin | ExpertType::Swift | ExpertType::C | ExpertType::Cpp |
            ExpertType::CSharp | ExpertType::Ruby | ExpertType::PHP | ExpertType::Dart |
            ExpertType::R | ExpertType::Scala | ExpertType::Elixir | ExpertType::Clojure |
            ExpertType::Haskell | ExpertType::Lua | ExpertType::Perl |
            ExpertType::Zig | ExpertType::Nim | ExpertType::OCaml | ExpertType::FSharp => "语言",
            // 前端
            ExpertType::React | ExpertType::Vue | ExpertType::Angular |
            ExpertType::Svelte | ExpertType::NextJS | ExpertType::Nuxt |
            ExpertType::SolidJS | ExpertType::Qwik | ExpertType::ReactNative |
            ExpertType::Flutter | ExpertType::Tauri | ExpertType::Electron |
            ExpertType::CSS | ExpertType::Tailwind | ExpertType::WebAssembly => "前端",
            // 后端
            ExpertType::REST | ExpertType::GraphQL | ExpertType::gRPC |
            ExpertType::Microservice | ExpertType::Serverless | ExpertType::EventDriven |
            ExpertType::CQRS | ExpertType::MessageQueue | ExpertType::Redis |
            ExpertType::Nginx | ExpertType::Apache | ExpertType::Lambda |
            ExpertType::Cloudflare => "后端",
            // 数据
            ExpertType::ML | ExpertType::DeepLearning | ExpertType::DataEngineering |
            ExpertType::DataAnalysis | ExpertType::DataVisualization |
            ExpertType::NLP | ExpertType::CV | ExpertType::ReinforcementLearning |
            ExpertType::MLOps | ExpertType::VectorDB => "数据",
            // DevOps
            ExpertType::Kubernetes | ExpertType::Docker | ExpertType::Terraform |
            ExpertType::Ansible | ExpertType::AWS | ExpertType::GCP | ExpertType::Azure |
            ExpertType::CiCd | ExpertType::Prometheus | ExpertType::Grafana |
            ExpertType::ELK | ExpertType::Vault | ExpertType::Consul |
            ExpertType::Helm | ExpertType::ArgoCD => "DevOps",
            // 数据库
            ExpertType::PostgreSQL | ExpertType::MySQL | ExpertType::MongoDB |
            ExpertType::Neo4j | ExpertType::Elasticsearch | ExpertType::ClickHouse |
            ExpertType::Cassandra | ExpertType::DynamoDB | ExpertType::SQLite |
            ExpertType::TimescaleDB => "数据库",
            // 安全/质量
            ExpertType::PenTest | ExpertType::AppSec | ExpertType::ThreatModeling |
            ExpertType::OWASP | ExpertType::LoadTesting | ExpertType::ChaosEngineering |
            ExpertType::CodeQuality | ExpertType::TechDebt |
            ExpertType::Accessibility | ExpertType::Internationalization => "安全/质量",
            // 领域
            ExpertType::GameDev | ExpertType::Embedded | ExpertType::IoT |
            ExpertType::Blockchain | ExpertType::NFT | ExpertType::DeFi |
            ExpertType::Web3 | ExpertType::RPA | ExpertType::LowCode | ExpertType::NoCode => "领域",
            // 扩展槽位
            _ => "扩展",
        }
    }
}

/// 专家 Agent 描述
#[derive(Debug, Clone)]
pub struct Expert {
    pub id: String,
    pub name: String,
    pub expert_type: ExpertType,
    pub description: String,
    pub specialty: Vec<String>,
    pub tools: Vec<String>,
    pub max_concurrent: usize,
}

impl Expert {
    pub fn new(expert_type: ExpertType) -> Self {
        // 工具集
        let coding_tools = vec!["read_file", "write_file", "run_command", "search_files"];
        let review_tools = vec!["read_file", "git_diff", "search_files", "grep"];
        let data_tools = vec!["read_file", "write_file", "run_command", "list_directory"];
        let read_only_tools = vec!["read_file", "search_files", "grep", "list_directory"];

        match expert_type {
            // ── 基础角色 ───────────────────────────────────────
            ExpertType::Coder => Self {
                id: "coder".into(), name: "通用编码专家".into(), expert_type,
                description: "通用编码专家，擅长各类语言的代码生成、编辑、调试、重构".into(),
                specialty: vec!["代码生成".into(), "代码编辑".into(), "调试".into(), "重构".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 4,
            },
            ExpertType::Frontend => Self {
                id: "frontend".into(), name: "前端开发专家".into(), expert_type,
                description: "专注前端开发，React/Vue/Angular，精通 CSS/TypeScript/动画".into(),
                specialty: vec!["React".into(), "Vue".into(), "TypeScript".into(), "CSS动画".into(), "组件设计".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Backend => Self {
                id: "backend".into(), name: "后端开发专家".into(), expert_type,
                description: "专注后端开发，API设计、数据库、性能优化、高并发".into(),
                specialty: vec!["API设计".into(), "Rust".into(), "Go".into(), "性能优化".into(), "高并发".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Security => Self {
                id: "security".into(), name: "安全专家".into(), expert_type,
                description: "专注安全审计、漏洞检测、渗透测试、安全加固".into(),
                specialty: vec!["安全审计".into(), "漏洞检测".into(), "渗透测试".into(), "OWASP".into(), "加密".into()],
                tools: review_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 1,
            },
            ExpertType::Tester => Self {
                id: "tester".into(), name: "测试专家".into(), expert_type,
                description: "专注测试，单元测试、集成测试、端到端测试、测试覆盖".into(),
                specialty: vec!["单元测试".into(), "集成测试".into(), "E2E测试".into(), "测试覆盖".into(), "TDD".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Reviewer => Self {
                id: "reviewer".into(), name: "代码审查专家".into(), expert_type,
                description: "专注代码审查，质量把关、风格规范、性能审查".into(),
                specialty: vec!["代码审查".into(), "代码规范".into(), "性能审查".into(), "安全审查".into()],
                tools: review_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Architect => Self {
                id: "architect".into(), name: "架构师".into(), expert_type,
                description: "专注系统架构，微服务、设计模式、技术选型、分布式系统".into(),
                specialty: vec!["系统架构".into(), "微服务".into(), "设计模式".into(), "技术选型".into(), "分布式".into()],
                tools: read_only_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 1,
            },
            ExpertType::DevOps => Self {
                id: "devops".into(), name: "DevOps工程师".into(), expert_type,
                description: "专注 DevOps，CiCd、K8s、Docker、监控、日志".into(),
                specialty: vec!["CiCd".into(), "Kubernetes".into(), "Docker".into(), "监控".into(), "日志".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Database => Self {
                id: "database".into(), name: "数据库专家".into(), expert_type,
                description: "专注数据库设计、SQL优化、NoSQL、图数据库".into(),
                specialty: vec!["SQL优化".into(), "NoSQL".into(), "图数据库".into(), "数据库设计".into(), "备份恢复".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Mobile => Self {
                id: "mobile".into(), name: "移动开发专家".into(), expert_type,
                description: "专注移动端开发，iOS/Android/React Native/Flutter".into(),
                specialty: vec!["iOS".into(), "Android".into(), "React Native".into(), "Flutter".into(), "移动端性能".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Data => Self {
                id: "data".into(), name: "数据工程师".into(), expert_type,
                description: "专注数据工程，ETL、数据管道、大数据、数据分析".into(),
                specialty: vec!["ETL".into(), "数据管道".into(), "大数据".into(), "数据分析".into(), "数据可视化".into()],
                tools: data_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::API => Self {
                id: "api".into(), name: "API专家".into(), expert_type,
                description: "专注 API 设计，REST/GraphQL/gRPC、API 版本管理".into(),
                specialty: vec!["REST API".into(), "GraphQL".into(), "gRPC".into(), "API版本".into(), "文档".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Testing => Self {
                id: "testing".into(), name: "测试工程师".into(), expert_type,
                description: "专注测试工程，测试策略、测试平台、自动化测试".into(),
                specialty: vec!["测试策略".into(), "自动化测试".into(), "测试平台".into(), "测试数据".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Performance => Self {
                id: "performance".into(), name: "性能工程师".into(), expert_type,
                description: "专注性能优化，Profiling、缓存优化、数据库调优".into(),
                specialty: vec!["Profiling".into(), "缓存优化".into(), "数据库调优".into(), "性能测试".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 1,
            },

            // ── 语言专家 ───────────────────────────────────────
            ExpertType::Rust => Self {
                id: "rust".into(), name: "Rust 专家".into(), expert_type,
                description: "Rust 语言专家，内存安全、并发、所有权系统".into(),
                specialty: vec!["Rust".into(), "所有权".into(), "生命周期".into(), "并发".into(), "WASM".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Go => Self {
                id: "go".into(), name: "Go 专家".into(), expert_type,
                description: "Go 语言专家，并发编程、微服务、云原生".into(),
                specialty: vec!["Go".into(), "Goroutine".into(), "CSP".into(), "微服务".into(), "云原生".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Python => Self {
                id: "python".into(), name: "Python 专家".into(), expert_type,
                description: "Python 语言专家，数据科学、Web、自动化脚本".into(),
                specialty: vec!["Python".into(), "数据科学".into(), "Web".into(), "自动化".into(), "ML".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::TypeScript => Self {
                id: "typescript".into(), name: "TypeScript 专家".into(), expert_type,
                description: "TypeScript 语言专家，类型系统、前后端开发".into(),
                specialty: vec!["TypeScript".into(), "类型推导".into(), "泛型".into(), "类型体操".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::JavaScript => Self {
                id: "javascript".into(), name: "JavaScript 专家".into(), expert_type,
                description: "JavaScript 语言专家，ES2024+、Node.js、前端".into(),
                specialty: vec!["JavaScript".into(), "ES2024".into(), "Node.js".into(), "前端".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Java => Self {
                id: "java".into(), name: "Java 专家".into(), expert_type,
                description: "Java 语言专家，Spring Boot、微服务、企业级开发".into(),
                specialty: vec!["Java".into(), "Spring Boot".into(), "JVM".into(), "微服务".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Kotlin => Self {
                id: "kotlin".into(), name: "Kotlin 专家".into(), expert_type,
                description: "Kotlin 语言专家，Android 开发、协程".into(),
                specialty: vec!["Kotlin".into(), "Android".into(), "协程".into(), "Spring".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Swift => Self {
                id: "swift".into(), name: "Swift 专家".into(), expert_type,
                description: "Swift 语言专家，iOS/macOS 开发、SwiftUI".into(),
                specialty: vec!["Swift".into(), "iOS".into(), "macOS".into(), "SwiftUI".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::Cpp => Self {
                id: "cpp".into(), name: "C++ 专家".into(), expert_type,
                description: "C++ 语言专家，高性能、嵌入式、游戏引擎".into(),
                specialty: vec!["C++".into(), "RAII".into(), "模板".into(), "高性能".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            ExpertType::C => Self {
                id: "c".into(), name: "C 语言专家".into(), expert_type,
                description: "C 语言专家，嵌入式、系统编程、操作系统".into(),
                specialty: vec!["C".into(), "嵌入式".into(), "系统编程".into(), "POSIX".into()],
                tools: coding_tools.iter().map(|s| s.to_string()).collect(), max_concurrent: 2,
            },
            // 剩余语言 — 简化版
            e => {
                let id = e.to_string();
                let name = format!("{:?}专家", e);
                let desc = format!("{:?}领域专家", e);
                Self {
                    id: id.clone(),
                    name,
                    expert_type: e,
                    description: desc,
                    specialty: vec![id],
                    tools: coding_tools.iter().map(|s| s.to_string()).collect(),
                    max_concurrent: 1,
                }
            }
        }
    }
}

/// 专家注册表
pub struct ExpertRegistry {
    experts: std::collections::HashMap<String, Expert>,
}

impl Default for ExpertRegistry {
    fn default() -> Self { Self::new() }
}

impl ExpertRegistry {
    pub fn new() -> Self {
        let mut experts = std::collections::HashMap::new();
        for et in ExpertType::all() {
            let expert = Expert::new(et);
            experts.insert(expert.id.clone(), expert);
        }
        Self { experts }
    }

    pub fn get(&self, id: &str) -> &Expert {
        self.experts.get(id).unwrap_or_else(|| self.experts.get("coder").unwrap())
    }

    /// 根据任务内容智能选择专家
    pub fn select(&self, task: &str) -> String {
        let t = task.to_lowercase();

        // 精确匹配
        if t.contains("rust") && !t.contains("rust+") { return "rust".into(); }
        if t.contains("go ") || t.contains("golang") { return "go".into(); }
        if t.contains("python") || t.contains("django") || t.contains("flask") { return "python".into(); }
        if t.contains("typescript") || t.contains(" ts ") { return "typescript".into(); }
        if t.contains("javascript") || t.contains(" js ") { return "javascript".into(); }
        if t.contains("java ") && !t.contains("javascript") { return "java".into(); }
        if t.contains("kotlin") { return "kotlin".into(); }
        if t.contains("swift ") { return "swift".into(); }
        if t.contains("c++") || t.contains("cpp") { return "cpp".into(); }
        if t.contains("c#") || t.contains("csharp") { return "csharp".into(); }

        // 前端
        if t.contains("react") { return "react".into(); }
        if t.contains("vue") || t.contains("nuxt") { return "vue".into(); }
        if t.contains("angular") { return "angular".into(); }
        if t.contains("svelte") { return "svelte".into(); }
        if t.contains("nextjs") || t.contains("next.js") { return "nextjs".into(); }
        if t.contains("tailwind") { return "tailwind".into(); }
        if t.contains("css") || t.contains("样式") || t.contains("前端") { return "frontend".into(); }
        if t.contains("flutter") || t.contains("移动端") || t.contains("手机") || t.contains("app") { return "mobile".into(); }

        // 后端/数据
        if t.contains("api") || t.contains("rest") || t.contains("graphql") || t.contains("grpc") { return "api".into(); }
        if t.contains("微服务") || t.contains("microservice") { return "microservice".into(); }
        if t.contains("serverless") || t.contains("lambda") { return "serverless".into(); }
        if t.contains("数据库") || t.contains("sql") || t.contains("mysql") || t.contains("postgresql") || t.contains("mongodb") { return "database".into(); }
        if t.contains("redis") || t.contains("缓存") { return "redis".into(); }
        if t.contains("消息队列") || t.contains("mq") || t.contains("kafka") { return "messagequeue".into(); }

        // 安全
        if t.contains("安全") || t.contains("漏洞") || t.contains("渗透") || t.contains("security") || t.contains("owasp") { return "security".into(); }

        // 测试
        if t.contains("测试") && !t.contains("单元测试") { return "tester".into(); }
        if t.contains("单元测试") || t.contains("集成测试") { return "testing".into(); }

        // 架构/重构
        if t.contains("架构") || t.contains("设计模式") { return "architect".into(); }
        if t.contains("review") || t.contains("审查") || t.contains("代码质量") { return "reviewer".into(); }

        // DevOps
        if t.contains("k8s") || t.contains("kubernetes") || t.contains("docker") { return "devops".into(); }
        if t.contains("ci/cd") || t.contains("github action") || t.contains("gitlab ci") { return "cicd".into(); }
        if t.contains("aws") || t.contains("azure") || t.contains("gcp") || t.contains("云") { return "devops".into(); }
        if t.contains("terraform") || t.contains("基础设施") || t.contains("iac") { return "devops".into(); }

        // 数据/ML
        if t.contains("机器学习") || t.contains("ml") || t.contains("ai") || t.contains("模型") || t.contains("训练") { return "ml".into(); }
        if t.contains("数据") || t.contains("etl") || t.contains("数据管道") { return "data".into(); }

        // 性能
        if t.contains("性能") || t.contains("优化") || t.contains("profiling") || t.contains("慢查询") { return "performance".into(); }

        // 默认
        "coder".into()
    }

    pub fn all(&self) -> Vec<&Expert> {
        self.experts.values().collect()
    }

    /// 按分类列出专家
    pub fn by_category(&self) -> std::collections::HashMap<&'static str, Vec<&Expert>> {
        let mut map: std::collections::HashMap<&str, Vec<&Expert>> = std::collections::HashMap::new();
        for e in self.experts.values() {
            map.entry(e.expert_type.category())
                .or_default()
                .push(e);
        }
        map
    }
}
