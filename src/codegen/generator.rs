//! 组件级代码生成器
//!
//! LLM 驱动：从自然语言描述生成具体代码组件
//! 支持：API endpoint / Data model / Service / CLI tool / Web component

use crate::error::Result;
use crate::llm::{Client as LlmClient, LlmRequest, Message};

/// 组件类型
#[derive(Debug, Clone)]
pub enum ComponentKind {
    ApiEndpoint,
    DataModel,
    Service,
    Cli,
    WebComponent,
    Migration,
    Config,
    Test,
}

impl ComponentKind {
    pub fn infer_from(description: &str) -> Self {
        let desc = description.to_lowercase();
        if desc.contains("api") || desc.contains("接口") || desc.contains("endpoint") || desc.contains("rest") {
            ComponentKind::ApiEndpoint
        } else if desc.contains("模型") || desc.contains("entity") || desc.contains("schema") || desc.contains("model") {
            ComponentKind::DataModel
        } else if desc.contains("前端") || desc.contains("页面") || desc.contains("ui") || desc.contains("component") {
            ComponentKind::WebComponent
        } else if desc.contains("cli") || desc.contains("命令行") {
            ComponentKind::Cli
        } else if desc.contains("migration") || desc.contains("数据库") {
            ComponentKind::Migration
        } else if desc.contains("配置") || desc.contains("config") {
            ComponentKind::Config
        } else if desc.contains("test") || desc.contains("测试") {
            ComponentKind::Test
        } else {
            ComponentKind::Service
        }
    }

    pub fn name(&self, desc: &str) -> String {
        let base = desc.chars().filter(|c| c.is_alphanumeric()).take(20).collect::<String>().to_lowercase();
        format!("{:?}", self).to_lowercase() + "_" + &base
    }
}

#[derive(Debug, Clone)]
pub struct ComponentRequest {
    pub kind: ComponentKind,
    pub description: String,
    pub language: String,
    pub framework: Option<String>,
    pub dependencies: Vec<String>,
}

pub struct ComponentGenerator {
    llm: Option<LlmClient>,
}

impl ComponentGenerator {
    pub fn new() -> Self { Self { llm: None } }

    pub fn with_llm(llm: LlmClient) -> Self { Self { llm: Some(llm) } }

    pub async fn generate_components(
        &self,
        description: &str,
        language: &str,
        framework: Option<&str>,
    ) -> Result<Vec<super::GeneratedFile>> {
        let requests = self.plan_components(description, language, framework).await?;
        tracing::info!("计划生成 {} 个组件", requests.len());

        let mut files = Vec::new();
        let mut generated_names = Vec::new();

        for req in requests {
            for dep in &req.dependencies {
                if !generated_names.iter().any(|n: &String| n.contains(dep)) {
                    tracing::warn!("依赖 {} 尚未生成，可能不完整", dep);
                }
            }
            let file = self.generate_single(&req).await?;
            generated_names.push(req.kind.name(&req.description));
            files.push(file);
        }
        Ok(files)
    }

    async fn plan_components(
        &self,
        description: &str,
        language: &str,
        _framework: Option<&str>,
    ) -> Result<Vec<ComponentRequest>> {
        let mut components = Vec::new();
        let desc_lower = description.to_lowercase();

        if desc_lower.contains("api") || desc_lower.contains("接口") || desc_lower.contains("rest") {
            components.push(ComponentRequest {
                kind: ComponentKind::ApiEndpoint,
                description: description.to_string(),
                language: language.to_string(),
                framework: None,
                dependencies: vec![],
            });
            if !desc_lower.contains("仅") && !desc_lower.contains("只") {
                components.push(ComponentRequest {
                    kind: ComponentKind::DataModel,
                    description: format!("{} 的数据模型", description),
                    language: language.to_string(),
                    framework: None,
                    dependencies: vec![],
                });
            }
        }

        if desc_lower.contains("前端") || desc_lower.contains("页面") || desc_lower.contains("ui")
            || desc_lower.contains("react") || desc_lower.contains("vue") || desc_lower.contains("component") {
            components.push(ComponentRequest {
                kind: ComponentKind::WebComponent,
                description: description.to_string(),
                language: "typescript".to_string(),
                framework: Some("react".to_string()),
                dependencies: vec![],
            });
        }

        if desc_lower.contains("cli") || desc_lower.contains("命令行") {
            components.push(ComponentRequest {
                kind: ComponentKind::Cli,
                description: description.to_string(),
                language: language.to_string(),
                framework: None,
                dependencies: vec![],
            });
        }

        if components.is_empty() {
            components.push(ComponentRequest {
                kind: ComponentKind::Service,
                description: description.to_string(),
                language: language.to_string(),
                framework: None,
                dependencies: vec![],
            });
        }

        if self.llm.is_none() || components.len() <= 2 {
            let reqs = self.template_components(description, language);
            return Ok(reqs);
        }

        let refined = self.refine_with_llm(description, language, components).await?;
        Ok(refined)
    }

    async fn refine_with_llm(
        &self,
        description: &str,
        language: &str,
        initial: Vec<ComponentRequest>,
    ) -> Result<Vec<ComponentRequest>> {
        let llm = self.llm.as_ref().ok_or_else(|| anyhow::anyhow!("no LLM client"))?;

        let prompt = format!(
            "分析以下需求，返回需要生成的代码组件列表（JSON数组）：\n需求: {}\n语言: {}\n\n返回格式（JSON数组，每项包含 kind/description）：\nkinds: api_endpoint, data_model, service, cli, web_component, migration, config, test\n\n只返回JSON，不要解释。示例：\n[{{\"kind\": \"data_model\", \"description\": \"User实体模型\"}}, {{\"kind\": \"api_endpoint\", \"description\": \"用户CRUD API\"}}]",
            description, language
        );

        let request = LlmRequest {
            model: "auto".to_string(),
            messages: vec![Message::user(&prompt)],
            temperature: Some(0.3),
            max_tokens: Some(500),
            stream: false,
            tools: None,
        };

        match llm.complete(request).await {
            Ok(resp) => {
                if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(&resp.content) {
                    let refined: Vec<ComponentRequest> = parsed.into_iter().filter_map(|v| {
                        let kind_str = v.get("kind")?.as_str()?;
                        let desc = v.get("description")?.as_str()?.to_string();
                        let kind = match kind_str {
                            "api_endpoint" => ComponentKind::ApiEndpoint,
                            "data_model" => ComponentKind::DataModel,
                            "service" => ComponentKind::Service,
                            "cli" => ComponentKind::Cli,
                            "web_component" => ComponentKind::WebComponent,
                            "migration" => ComponentKind::Migration,
                            "config" => ComponentKind::Config,
                            "test" => ComponentKind::Test,
                            _ => return None,
                        };
                        Some(ComponentRequest { kind, description: desc, language: language.to_string(), framework: None, dependencies: vec![] })
                    }).collect();
                    if !refined.is_empty() {
                        return Ok(refined);
                    }
                }
                tracing::warn!("LLM 规划解析失败，使用初始规划");
                Ok(initial)
            }
            Err(e) => {
                tracing::warn!("LLM 规划失败: {}，使用启发式规划", e);
                Ok(initial)
            }
        }
    }

    async fn generate_single(&self, req: &ComponentRequest) -> Result<super::GeneratedFile> {
        if let Some(llm) = &self.llm {
            let code = self.generate_with_llm(llm, req).await?;
            return Ok(super::GeneratedFile {
                path: req.kind.default_path(&req.language),
                content: code,
                language: req.language.clone(),
                description: req.description.clone(),
            });
        }
        Ok(self.generate_template(req))
    }

    async fn generate_with_llm(&self, llm: &LlmClient, req: &ComponentRequest) -> Result<String> {
        let fw_hint = req.framework.as_ref().map(|f| format!("框架: {}\n", f)).unwrap_or_default();
        let fw_prompt = req.framework.as_ref().map(|f| format!("框架: {}\n", f)).unwrap_or_default();

        let prompt = format!(
            "为以下需求生成完整的 {} 代码。只返回代码本身，不要解释。\n\n需求: {}\n组件类型: {:?}\n{}\n要求：\n- 代码完整可编译/运行\n- 包含必要的 imports\n- 遵循 {} 最佳实践\n- 添加必要的类型标注\n- 包含基本的错误处理",
            req.language,
            req.description,
            req.kind,
            fw_prompt,
            req.language
        );

        let request = LlmRequest {
            model: "auto".to_string(),
            messages: vec![Message::user(&prompt)],
            temperature: Some(0.2),
            max_tokens: Some(2000),
            stream: false,
            tools: None,
        };

        let response = llm.complete(request).await?;
        Ok(response.content)
    }

    fn template_components(&self, description: &str, language: &str) -> Vec<ComponentRequest> {
        let kind = ComponentKind::infer_from(description);
        vec![ComponentRequest {
            kind,
            description: description.to_string(),
            language: language.to_string(),
            framework: None,
            dependencies: vec![],
        }]
    }

    fn generate_template(&self, req: &ComponentRequest) -> super::GeneratedFile {
        super::GeneratedFile {
            path: req.kind.default_path(&req.language),
            content: req.kind.generate_template(&req.language),
            language: req.language.clone(),
            description: req.description.clone(),
        }
    }
}

impl Default for ComponentGenerator {
    fn default() -> Self { Self::new() }
}

impl ComponentKind {
    pub fn default_path(&self, language: &str) -> String {
        match (self, language) {
            (ComponentKind::ApiEndpoint, "rust") => "src/api/mod.rs".into(),
            (ComponentKind::DataModel, "rust") => "src/models/mod.rs".into(),
            (ComponentKind::Service, "rust") => "src/service/mod.rs".into(),
            (ComponentKind::Cli, "rust") => "src/main.rs".into(),
            (ComponentKind::Test, "rust") => "tests/basic_test.rs".into(),
            (ComponentKind::WebComponent, "typescript") => "src/components/Component.tsx".into(),
            (ComponentKind::ApiEndpoint, "typescript") => "src/api/index.ts".into(),
            (ComponentKind::DataModel, "typescript") => "src/types/index.ts".into(),
            (ComponentKind::Service, "typescript") => "src/services/index.ts".into(),
            (ComponentKind::Test, "typescript") => "src/__tests__/index.test.ts".into(),
            (ComponentKind::Service, "python") => "src/service.py".into(),
            (ComponentKind::ApiEndpoint, "python") => "src/api.py".into(),
            (ComponentKind::DataModel, "python") => "src/models.py".into(),
            (ComponentKind::Cli, "python") => "src/main.py".into(),
            (ComponentKind::Test, "python") => "tests/test_main.py".into(),
            (ComponentKind::ApiEndpoint, "golang") => "internal/handler/api.go".into(),
            (ComponentKind::DataModel, "golang") => "internal/model/model.go".into(),
            (ComponentKind::Service, "golang") => "internal/service/service.go".into(),
            (ComponentKind::Test, "golang") => "internal/model/model_test.go".into(),
            (ComponentKind::Config, _) => "config.yaml".into(),
            _ => format!("src/{}.{}", Self::filename_str(self), language),
        }
    }

    fn filename_str(kind: &ComponentKind) -> &'static str {
        match kind {
            ComponentKind::ApiEndpoint => "api",
            ComponentKind::DataModel => "model",
            ComponentKind::Service => "service",
            ComponentKind::Cli => "main",
            ComponentKind::WebComponent => "component",
            ComponentKind::Migration => "migration",
            ComponentKind::Config => "config",
            ComponentKind::Test => "test",
        }
    }

    pub fn generate_template(&self, language: &str) -> String {
        match (self, language) {
            // Rust templates
            (ComponentKind::ApiEndpoint, "rust") => {
                "use axum::{extract::{Path, State, Json}, routing::{get, post}, Router};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self { success: true, data: Some(data), message: None }
    }
    pub fn error(msg: impl Into<String>) -> ApiResponse<()> {
        Self { success: false, data: None, message: Some(msg.into()) }
    }
}

#[derive(Clone)]
struct AppState;

#[utoipa::path(get, path = \"/health\", responses((status = 200, body = ApiResponse<()>)))]
async fn health() -> Json<ApiResponse<()>> {
    Json(ApiResponse::success(()))
}

// TODO: implement API handlers
".into()
            }
            (ComponentKind::DataModel, "rust") => {
                "use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Option<i64>,
    pub name: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Entity {
    pub fn new(name: impl Into<String>) -> Self {
        Self { id: None, name: name.into(), created_at: None }
    }
}
".into()
            }
            (ComponentKind::Service, "rust") => {
                "use crate::error::Result;

pub struct Service;

impl Service {
    pub fn new() -> Self { Self }
    pub async fn do_something(&self) -> Result<String> {
        Ok(\"done\".to_string())
    }
}
".into()
            }
            (ComponentKind::Cli, "rust") => {
                "use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = \"acode\")]
#[command(version = \"0.1.0\")]
struct Cli {
    #[arg(short, long)]
    task: String,
    #[arg(short, long, default_value = \".\")]
    workdir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    println!(\"任务: {}\", cli.task);
    Ok(())
}
".into()
            }
            (ComponentKind::Test, "rust") => {
                "#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
".into()
            }
            // TypeScript templates
            (ComponentKind::WebComponent, "typescript") => {
                "export interface Props {\n  className?: string;\n}\n\nexport function Component({ className }: Props) {\n  return (\n    <div className={className}>\n      {/* TODO: implement component */}\n    </div>\n  );\n}\n".into()
            }
            (ComponentKind::ApiEndpoint, "typescript") => {
                "export interface ApiResponse<T> {\n  success: boolean;\n  data?: T;\n  message?: string;\n}\n\nexport async function apiGet<T>(url: string): Promise<ApiResponse<T>> {\n  const res = await fetch(url);\n  return res.json();\n}\n\nexport async function apiPost<T>(url: string, body: unknown): Promise<ApiResponse<T>> {\n  const res = await fetch(url, {\n    method: 'POST',\n    headers: { 'Content-Type': 'application/json' },\n    body: JSON.stringify(body),\n  });\n  return res.json();\n}\n".into()
            }
            (ComponentKind::DataModel, "typescript") => {
                "export interface Entity {\n  id?: number;\n  name: string;\n  createdAt?: Date;\n}\n\nexport function createEntity(name: string): Entity {\n  return { name, createdAt: new Date() };\n}\n".into()
            }
            (ComponentKind::Test, "typescript") => {
                "describe('Component', () => {\n  it('works', () => {\n    expect(1 + 1).toBe(2);\n  });\n});\n".into()
            }
            // Python templates
            (ComponentKind::Service, "python") => {
                "from dataclasses import dataclass\nfrom datetime import datetime\n\n\n@dataclass\nclass Entity:\n    name: str\n    id: int | None = None\n    created_at: datetime | None = None\n\n\ndef create_entity(name: str) -> Entity:\n    return Entity(name=name, created_at=datetime.utcnow())\n".into()
            }
            (ComponentKind::ApiEndpoint, "python") => {
                "from fastapi import FastAPI, HTTPException\nfrom pydantic import BaseModel\n\napp = FastAPI()\n\n\nclass Item(BaseModel):\n    name: str\n    description: str | None = None\n\n\n# TODO: implement API endpoints\n".into()
            }
            (ComponentKind::Cli, "python") => {
                "#!/usr/bin/env python3\nimport argparse\n\n\ndef main():\n    parser = argparse.ArgumentParser(description='CLI Tool')\n    parser.add_argument('task', help='Task description')\n    args = parser.parse_args()\n    print(f'Task: {args.task}')\n\n\nif __name__ == '__main__':\n    main()\n".into()
            }
            (ComponentKind::Test, "python") => {
                "import pytest\n\n\ndef test_example():\n    assert 1 + 1 == 2\n".into()
            }
            // Go templates
            (ComponentKind::ApiEndpoint, "golang") => {
                "package handler\n\nimport (\n    \"net/http\"\n    \"github.com/gin-gonic/gin\"\n)\n\ntype Response struct {\n    Success bool        `json:\"success\"`\n    Data    interface{} `json:\"data,omitempty\"`\n    Message string      `json:\"message,omitempty\"`\n}\n\n// TODO: implement handlers\n".into()
            }
            (ComponentKind::DataModel, "golang") => {
                "package model\n\ntype Entity struct {\n    ID   int64  `json:\"id\"`\n    Name string `json:\"name\"`\n}\n\n// TODO: implement methods\n".into()
            }
            (ComponentKind::Service, "golang") => {
                "package service\n\n// TODO: implement business logic\n".into()
            }
            _ => format!("// Generated by ACoder - {} {} template\n// TODO: implement", language, Self::filename_str(self)),
        }
    }
}
