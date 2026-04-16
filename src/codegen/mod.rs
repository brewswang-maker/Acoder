//! # 全场景代码生成引擎 (Phase 1: Week 5-10)
//!
//! 覆盖前后端 / Rust / Go / TS / Python 全场景代码生成
//! 借鉴 claw-code 的项目骨架生成 + zed 的代码片段系统
//!
//! ## 架构
//! ```text
//! CodegenEngine
//! ├── LanguageDetector     — 技术栈自动检测
//! ├── TemplateEngine       — 多语言项目模板（20+ 模板）
//! ├── ComponentGenerator  — LLM 驱动的组件级代码生成
//! ├── DependencyResolver   — 依赖解析与版本锁定
//! └── ProjectScaffold     — 项目脚手架（基于 scaffold.rs 扩展）
//! ```

pub mod detector;
pub mod templates;
pub mod generator;
pub mod resolver;

pub use detector::LanguageDetector;
pub use templates::{ProjectTemplateRegistry, TemplateCategory};
pub use generator::{ComponentGenerator, ComponentKind, ComponentRequest};
pub use resolver::DependencyResolver;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::error::Result;

/// 代码生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodegenConfig {
    /// 目标语言
    pub language: String,
    /// 框架（可选）
    pub framework: Option<String>,
    /// 项目名称
    pub project_name: String,
    /// 是否包含测试
    pub with_tests: bool,
    /// 是否包含 CI/CD
    pub with_ci: bool,
    /// 是否包含 Docker
    pub with_docker: bool,
    /// 代码风格
    pub style: CodeStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CodeStyle {
    #[default]
    Standard,
    Minimal,
    Enterprise,
    Microservice,
}

/// 生成的文件
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    pub language: String,
    pub description: String,
}

/// 代码生成引擎
pub struct CodegenEngine {
    templates: ProjectTemplateRegistry,
    generator: ComponentGenerator,
    resolver: DependencyResolver,
}

impl CodegenEngine {
    pub fn new() -> Self {
        Self {
            templates: ProjectTemplateRegistry::new(),
            generator: ComponentGenerator::new(),
            resolver: DependencyResolver::new(),
        }
    }

    /// 使用 LLM client 创建（推荐）
    pub fn with_llm(llm: crate::llm::Client) -> Self {
        Self {
            templates: ProjectTemplateRegistry::new(),
            generator: ComponentGenerator::with_llm(llm),
            resolver: DependencyResolver::new(),
        }
    }

    /// 从自然语言描述生成项目
    pub async fn generate_from_description(
        &self,
        description: &str,
        output_dir: PathBuf,
        config: CodegenConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut files = Vec::new();

        // 1. 检测技术栈（如果未指定）
        let language = if config.language.is_empty() {
            LanguageDetector::detect(description).primary
        } else {
            config.language.clone()
        };

        let framework = config.framework.clone();

        tracing::info!(
            "Codegen 开始 | 语言: {} | 框架: {:?} | 项目: {}",
            language, framework, config.project_name
        );

        // 2. 选择并渲染项目模板
        let template = self.templates.select(&language, framework.as_deref());
        let template_files = template.render(&config.project_name, &config);
        tracing::info!("模板生成 {} 个文件", template_files.len());
        files.extend(template_files);

        // 3. 生成核心组件（LLM 驱动）
        let components = self.generator
            .generate_components(description, &language, framework.as_deref())
            .await?;
        tracing::info!("组件生成 {} 个文件", components.len());
        files.extend(components);

        // 4. 解析并锁定依赖
        let deps = self.resolver.resolve(&language, framework.as_deref())?;
        tracing::info!("依赖生成 {} 个文件", deps.len());
        for dep_file in deps {
            files.push(GeneratedFile {
                path: dep_file.path.clone(),
                content: dep_file.content.clone(),
                language: dep_file.language.clone(),
                description: dep_file.description.clone(),
            });
        }

        // 5. 生成测试（如果启用）
        if config.with_tests {
            let tests = self.generate_tests(&language, &files).await?;
            tracing::info!("测试生成 {} 个文件", tests.len());
            files.extend(tests);
        }

        // 6. 写入文件
        let mut written = 0usize;
        for file in &files {
            let full_path = output_dir.join(&file.path);
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&full_path, &file.content).await?;
            tracing::info!("  生成: {}", file.path);
            written += 1;
        }

        tracing::info!(
            "✅ 项目生成完成 | {} 个文件 | 语言: {} | 路径: {}",
            written, language, output_dir.display()
        );

        Ok(files)
    }

    /// 生成测试文件
    async fn generate_tests(&self, language: &str, files: &[GeneratedFile]) -> Result<Vec<GeneratedFile>> {
        let mut tests = Vec::new();

        // 为每个源文件生成对应的测试文件
        for file in files {
            if file.path.contains("_test") || file.path.contains(".test.") || file.path.contains("test_") {
                continue; // 跳过已经是测试的文件
            }
            if file.language == "text" || file.language == "json" || file.language == "toml" {
                continue; // 跳过配置文件
            }

            let test_file = self.generate_test_for_file(file, language);
            if let Some(tf) = test_file {
                tests.push(tf);
            }
        }

        Ok(tests)
    }

    fn generate_test_for_file(&self, source: &GeneratedFile, language: &str) -> Option<GeneratedFile> {
        let test_path = match language {
            "rust" => {
                let base = source.path.replace(".rs", "_test.rs");
                if source.path.ends_with("lib.rs") {
                    format!("tests/{}_test.rs", source.path.trim_start_matches("src/").replace("/", "_"))
                } else {
                    format!("tests/{}_test.rs", base.trim_start_matches("src/").replace("/", "_"))
                }
            }
            "typescript" | "javascript" => {
                let base = source.path.trim_end_matches(".ts").trim_end_matches(".tsx").trim_end_matches(".js");
                format!("{}.test.ts", base)
            }
            "python" => {
                let base = source.path.trim_end_matches(".py");
                if base.starts_with("src/") {
                    format!("tests/{}_test.py", base.trim_start_matches("src/"))
                } else {
                    format!("tests/{}_test.py", base)
                }
            }
            "golang" => {
                format!("{}_test.go", source.path.trim_end_matches(".go"))
            }
            _ => return None,
        };

        let test_content = match language {
            "rust" => {
                let mod_name = source.path.replace("src/", "").replace(".rs", "").replace("/", "::");
                format!(
                    r#"#[cfg(test)]
mod {}_tests {{
    use super::*;

    #[test]
    fn placeholder_test() {{
        // TODO: 编写 {} 的测试
        assert!(true);
    }}
}}
"#,
                    mod_name.replace("::", "_"),
                    mod_name
                )
            }
            "typescript" => {
                format!(
                    r#"import {{}} from './{}';

describe('{}', () => {{
    it('placeholder test', () => {{
        expect(true).toBe(true);
    }});
}});
"#,
                    source.path.trim_end_matches(".ts"),
                    source.path.split('/').last().unwrap().trim_end_matches(".ts")
                )
            }
            "python" => {
                let fn_name = source.path.split('/').last().unwrap().trim_end_matches(".py");
                format!(
                    r#"import pytest
import sys
sys.path.insert(0, '../src')

def test_placeholder():
    # TODO: 编写 {} 的测试
    assert True
"#,
                    fn_name
                )
            }
            _ => return None,
        };

        Some(GeneratedFile {
            path: test_path,
            content: test_content,
            language: language.to_string(),
            description: format!("{} 的测试", source.path),
        })
    }

    /// 列出所有可用模板
    pub fn list_templates(&self) -> Vec<TemplateCategory> {
        self.templates.categories()
    }

    /// 获取支持的编程语言
    pub fn supported_languages() -> &'static [&'static str] {
        &["rust", "typescript", "javascript", "python", "golang", "java", "kotlin", "swift", "cpp", "csharp"]
    }
}

impl Default for CodegenEngine {
    fn default() -> Self { Self::new() }
}
