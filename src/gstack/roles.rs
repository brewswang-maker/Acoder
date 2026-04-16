//! gstack 角色定义
//!
//! 每个 gstack skill 对应一个角色，角色定义其专业领域和工具集。

use serde::{Deserialize, Serialize};

/// gstack 角色体系（对应 gstack 的 23 个专家）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// CEO — 产品方向、需求澄清
    Ceo,
    /// Engineering Manager — 架构决策、技术债务
    EngineeringManager,
    /// Security Officer — OWASP + STRIDE 审计
    SecurityOfficer,
    /// QA Lead — 端到端测试、浏览器自动化
    QaLead,
    /// Designer — UI/UX 审查
    Designer,
    /// Release Engineer — CI/CD、PR 管理
    ReleaseEngineer,
    /// DevEx — 开发体验评审
    DevEx,
    /// Architect — 系统架构
    Architect,
    /// DevOps — 部署、监控
    DevOps,
    /// ProductManager — 产品策略
    ProductManager,
}

impl Role {
    /// 从 skill name 反推角色
    pub fn from_skill(skill: &str) -> Role {
        match skill {
            "office-hours" => Role::Ceo,
            "plan-ceo-review" => Role::Ceo,
            "plan-eng-review" => Role::EngineeringManager,
            "plan-design-review" => Role::Designer,
            "plan-devex-review" => Role::DevEx,
            "review" | "qa" | "qa-only" => Role::QaLead,
            "ship" | "land-and-deploy" | "freeze" | "unfreeze" => Role::ReleaseEngineer,
            "canary" | "benchmark" | "health" => Role::EngineeringManager,
            "investigate" => Role::EngineeringManager,
            "design-consultation" | "design-review" | "design-shotgun" => Role::Designer,
            "document-release" => Role::ReleaseEngineer,
            "checkpoint" | "learn" => Role::Ceo,
            "guard" | "cso" => Role::SecurityOfficer,
            "careful" => Role::SecurityOfficer,
            "autoplan" => Role::EngineeringManager,
            "browse" | "connect-chrome" => Role::DevEx,
            "devex-review" => Role::DevEx,
            "gstack-upgrade" => Role::DevEx,
            _ => Role::EngineeringManager,
        }
    }

    pub fn prompt_suffix(&self) -> &'static str {
        match self {
            Role::Ceo => "You are the CEO perspective. Focus on user outcomes, product-market fit, and whether this is worth building.",
            Role::EngineeringManager => "You are the Engineering Manager. Focus on architecture, technical debt, delivery risk, and team velocity.",
            Role::SecurityOfficer => "You are the Security Officer. Focus on OWASP Top 10, STRIDE threat model, and data safety.",
            Role::QaLead => "You are the QA Lead. Focus on test coverage, edge cases, browser automation, and ship readiness.",
            Role::Designer => "You are the Designer. Focus on visual polish, UX patterns, design system consistency, and user delight.",
            Role::ReleaseEngineer => "You are the Release Engineer. Focus on CI/CD, deployment safety, atomic commits, and rollback plans.",
            Role::DevEx => "You are the Developer Experience lead. Focus on tooling, scripts, automation, and DX metrics.",
            Role::Architect => "You are the Architect. Focus on system design, scalability, and long-term maintainability.",
            Role::DevOps => "You are the DevOps engineer. Focus on infrastructure, monitoring, and operational excellence.",
            Role::ProductManager => "You are the Product Manager. Focus on requirements clarity, prioritization, and success metrics.",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Role::Ceo => "CEO",
            Role::EngineeringManager => "Engineering Manager",
            Role::SecurityOfficer => "Security Officer",
            Role::QaLead => "QA Lead",
            Role::Designer => "Designer",
            Role::ReleaseEngineer => "Release Engineer",
            Role::DevEx => "Developer Experience",
            Role::Architect => "Architect",
            Role::DevOps => "DevOps",
            Role::ProductManager => "Product Manager",
        };
        write!(f, "{}", s)
    }
}
