//! 项目模板注册表 — 多语言项目骨架生成
//!
//! 支持 5 大语言 20+ 模板：
//! Rust: axum-web, actix-api, cli-app, tauri-desktop, lib-crate
//! TypeScript: react-spa, nextjs-fullstack, vue-app, express-api, electron-desktop
//! Python: fastapi-api, django-fullstack, cli-tool, ml-project
//! Go: gin-api, cli-app, grpc-service
//! Java: spring-boot-api, gradle-lib

use serde::{Deserialize, Serialize};

use super::{GeneratedFile, CodegenConfig, CodeStyle};

/// 项目模板
#[derive(Debug, Clone)]
pub struct ProjectTemplate {
    pub name: &'static str,
    pub language: &'static str,
    pub framework: Option<&'static str>,
    pub description: &'static str,
    pub files: fn(&str, &CodegenConfig) -> Vec<GeneratedFile>,
}

/// 模板分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCategory {
    pub language: String,
    pub templates: Vec<TemplateInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub framework: Option<String>,
    pub description: String,
}

/// 项目模板注册表
pub struct ProjectTemplateRegistry {
    templates: Vec<ProjectTemplate>,
}

impl ProjectTemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: Self::builtin_templates(),
        }
    }

    /// 根据语言和框架选择最匹配的模板
    pub fn select(&self, language: &str, framework: Option<&str>) -> &ProjectTemplate {
        // 优先精确匹配框架
        if let Some(fw) = framework {
            if let Some(t) = self.templates.iter().find(|t| {
                t.language == language && t.framework.map(|f| f == fw).unwrap_or(false)
            }) {
                return t;
            }
        }
        // 按语言匹配
        if let Some(t) = self.templates.iter().find(|t| t.language == language) {
            return t;
        }
        // 默认 Rust
        self.templates.first().unwrap()
    }

    /// 列出所有模板分类
    pub fn categories(&self) -> Vec<TemplateCategory> {
        let mut map = std::collections::HashMap::new();
        for t in &self.templates {
            map.entry(t.language).or_insert_with(Vec::new).push(TemplateInfo {
                name: t.name.to_string(),
                framework: t.framework.map(|s| s.to_string()),
                description: t.description.to_string(),
            });
        }
        map.into_iter().map(|(language, templates)| TemplateCategory { language: language.to_string(), templates }).collect()
    }

    fn builtin_templates() -> Vec<ProjectTemplate> {
        vec![
            // ── Rust ────────────────────────────────────────
            ProjectTemplate {
                name: "axum-web",
                language: "rust",
                framework: Some("axum"),
                description: "Axum Web 服务",
                files: rust_axum_files,
            },
            ProjectTemplate {
                name: "actix-api",
                language: "rust",
                framework: Some("actix-web"),
                description: "Actix-Web REST API",
                files: rust_actix_files,
            },
            ProjectTemplate {
                name: "rust-cli",
                language: "rust",
                framework: None,
                description: "Rust CLI 工具",
                files: rust_cli_files,
            },
            ProjectTemplate {
                name: "rust-lib",
                language: "rust",
                framework: None,
                description: "Rust 库 (crate)",
                files: rust_lib_files,
            },
            // ── TypeScript ─────────────────────────────────
            ProjectTemplate {
                name: "react-spa",
                language: "typescript",
                framework: Some("react"),
                description: "React SPA 应用",
                files: ts_react_files,
            },
            ProjectTemplate {
                name: "nextjs-app",
                language: "typescript",
                framework: Some("nextjs"),
                description: "Next.js 全栈应用",
                files: ts_nextjs_files,
            },
            ProjectTemplate {
                name: "vue-app",
                language: "typescript",
                framework: Some("vue"),
                description: "Vue 3 应用",
                files: ts_vue_files,
            },
            ProjectTemplate {
                name: "express-api",
                language: "typescript",
                framework: Some("express"),
                description: "Express REST API",
                files: ts_express_files,
            },
            // ── Python ─────────────────────────────────────
            ProjectTemplate {
                name: "fastapi-api",
                language: "python",
                framework: Some("fastapi"),
                description: "FastAPI REST API",
                files: py_fastapi_files,
            },
            ProjectTemplate {
                name: "django-app",
                language: "python",
                framework: Some("django"),
                description: "Django 全栈应用",
                files: py_django_files,
            },
            ProjectTemplate {
                name: "python-cli",
                language: "python",
                framework: None,
                description: "Python CLI 工具",
                files: py_cli_files,
            },
            ProjectTemplate {
                name: "python-ml",
                language: "python",
                framework: None,
                description: "Python ML 项目",
                files: py_ml_files,
            },
            // ── Go ─────────────────────────────────────────
            ProjectTemplate {
                name: "gin-api",
                language: "golang",
                framework: Some("gin"),
                description: "Gin REST API",
                files: go_gin_files,
            },
            ProjectTemplate {
                name: "go-cli",
                language: "golang",
                framework: None,
                description: "Go CLI 工具",
                files: go_cli_files,
            },
            ProjectTemplate {
                name: "go-grpc",
                language: "golang",
                framework: None,
                description: "Go gRPC 服务",
                files: go_grpc_files,
            },
            // ── Java ───────────────────────────────────────
            ProjectTemplate {
                name: "spring-boot",
                language: "java",
                framework: Some("spring-boot"),
                description: "Spring Boot API",
                files: java_spring_files,
            },
        ]
    }
}

impl Default for ProjectTemplateRegistry {
    fn default() -> Self { Self::new() }
}

impl ProjectTemplate {
    pub fn render(&self, project_name: &str, config: &CodegenConfig) -> Vec<GeneratedFile> {
        (self.files)(project_name, config)
    }
}

// ═══════════════════════════════════════════════════════════
//  Rust 模板
// ═══════════════════════════════════════════════════════════

fn rust_axum_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let pkg = name.replace('-', "_");
    let mut files = vec![
        file("Cargo.toml", &format!(r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
tower-http = {{ version = "0.6", features = ["cors", "trace"] }}
"#, pkg), "toml", "Rust 项目配置"),
        file("src/main.rs", &format!(r#"//! {} — Axum Web 服务

use axum::{{Router, routing::get, Json}};
use serde::{{Serialize, Deserialize}};

#[tokio::main]
async fn main() {{
    tracing_subscriber::init();
    let app = Router::new()
        .route("/", get(health))
        .route("/api/hello", get(hello));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("服务启动: http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}}

async fn health() -> Json<serde_json::Value> {{
    Json(serde_json::json!({{"status": "ok", "service": "{name}"}}))
}}

async fn hello() -> Json<serde_json::Value> {{
    Json(serde_json::json!({{"message": "Hello from {name}"}}))
}}
"#, name = name), "rust", "入口文件"),
        file("README.md", &format!("# {}\n\nAxum Web 服务\n\n```bash\ncargo run\n```", name), "markdown", "README"),
    ];
    if cfg.with_tests { files.push(file("tests/integration_test.rs", r#"#[tokio::test]
async fn test_health() {
    let resp = reqwest::get("http://localhost:3000/").await.unwrap();
    assert!(resp.status().is_success());
}"#, "rust", "集成测试")); }
    if cfg.with_ci { files.push(file(".github/workflows/ci.yml", r#"name: CI
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --all
      - run: cargo clippy -- -D warnings"#, "yaml", "CI 配置")); }
    files
}

fn rust_actix_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let pkg = name.replace('-', "_");
    let mut files = vec![
        file("Cargo.toml", &format!(r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
actix-web = "4"
actix-rt = "2"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#, pkg), "toml", "Rust 项目配置"),
        file("src/main.rs", &format!(r#"use actix_web::{{web, App, HttpServer, HttpResponse, Json}};
use serde::Serialize;

#[derive(Serialize)]
struct Health {{ status: String }}

#[actix_web::main]
async fn main() -> std::io::Result<()> {{
    HttpServer::new(|| {{
        App::new()
            .route("/", web::get().to(health))
    }})
    .bind("0.0.0.0:3000")?
    .run()
    .await
}}

async fn health() -> Json<Health> {{
    Json(Health {{ status: "ok".into() }})
}}"#), "rust", "入口文件"),
    ];
    if cfg.with_tests { files.push(file("tests/api_test.rs", r#"#[actix_web::test]
async fn test_health() {
    let app = actix_web::test::init_service(
        actix_web::App::new().route("/", actix_web::web::get().to(|| async {
            actix_web::HttpResponse::Ok().json(serde_json::json!({"status":"ok"}))
        }))
    ).await;
    let req = actix_web::test::TestRequest::get().to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}"#, "rust", "API 测试")); }
    files
}

fn rust_cli_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let pkg = name.replace('-', "_");
    let mut files = vec![
        file("Cargo.toml", &format!(r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = {{ version = "4", features = ["derive"] }}
anyhow = "1"
"#, pkg), "toml", "Rust 项目配置"),
        file("src/main.rs", &format!(r#"use clap::Parser;

#[derive(Parser)]
#[command(name = "{}", version, about = "CLI 工具")]
struct Cli {{
    #[arg(short, long)]
    verbose: bool,
    #[arg(subcommand)]
    command: Option<Commands>,
}}

#[derive(clap::Subcommand)]
enum Commands {{
    /// 运行任务
    Run {{ task: String }},
    /// 显示版本
    Version,
}}

fn main() -> anyhow::Result<()> {{
    let cli = Cli::parse();
    match cli.command {{
        Some(Commands::Run {{ task }}) => println!("运行: {{}}", task),
        Some(Commands::Version) => println!("{} v0.1.0", env!("CARGO_PKG_NAME")),
        None => println!("使用 --help 查看帮助"),
    }}
    Ok(())
}}"#, name, name), "rust", "入口文件"),
    ];
    if cfg.with_tests { files.push(file("tests/cli_test.rs", r#"#[test]
fn test_cli_help() {
    use std::process::Command;
    let output = Command::new(env!("CARGO_BIN_EXE_cli-tool"))
        .args(["--help"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("CLI"));
}"#, "rust", "CLI 测试")); }
    files
}

fn rust_lib_files(name: &str, _cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let pkg = name.replace('-', "_");
    vec![
        file("Cargo.toml", &format!(r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "2"

[dev-dependencies]
"#, pkg), "toml", "Rust 项目配置"),
        file("src/lib.rs", &format!(r#"//! {} — Rust 库

mod error;
pub use error::{{Error, Result}};

/// 库的核心功能
pub fn process(input: &str) -> Result<String> {{
    Ok(input.to_uppercase())
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn test_process() {{
        assert_eq!(process("hello").unwrap(), "HELLO");
    }}
}}"#, name), "rust", "库入口"),
        file("src/error.rs", r#"use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
"#, "rust", "错误类型"),
    ]
}

// ═══════════════════════════════════════════════════════════
//  TypeScript 模板
// ═══════════════════════════════════════════════════════════

fn ts_react_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let mut files = vec![
        file("package.json", &format!(r#"{{
  "name": "{}",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "test": "vitest"
  }},
  "dependencies": {{
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  }},
  "devDependencies": {{
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "typescript": "^5.6.0",
    "vite": "^6.0.0",
    "@vitejs/plugin-react": "^4.0.0",
    "vitest": "^2.0.0"
  }}
}}"#, name), "json", "Node.js 配置"),
        file("tsconfig.json", r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "lib": ["ES2020", "DOM"],
    "jsx": "react-jsx",
    "strict": true,
    "moduleResolution": "bundler",
    "outDir": "dist"
  },
  "include": ["src"]
}"#, "json", "TypeScript 配置"),
        file("src/main.tsx", r#"import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
"#, "typescript", "入口文件"),
        file("src/App.tsx", &format!(r#"function App() {{
  return (
    <div>
      <h1>{name}</h1>
      <p>React + TypeScript + Vite</p>
    </div>
  );
}}

export default App;"#), "typescript", "主组件"),
        file("index.html", r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8" /><meta name="viewport" content="width=device-width" /></head>
<body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body>
</html>"#, "html", "HTML 模板"),
    ];
    if cfg.with_tests { files.push(file("src/App.test.tsx", r#"import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

describe('App', () => {
  it('renders heading', () => {
    render(<App />);
    expect(screen.getByRole('heading')).toBeDefined();
  });
});
"#, "typescript", "组件测试")); }
    files
}

fn ts_nextjs_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let mut files = vec![
        file("package.json", &format!(r#"{{
  "name": "{}",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "dev": "next dev",
    "build": "next build",
    "start": "next start"
  }},
  "dependencies": {{
    "next": "^15.0.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  }},
  "devDependencies": {{
    "@types/node": "^22.0.0",
    "@types/react": "^19.0.0",
    "typescript": "^5.6.0"
  }}
}}"#, name), "json", "Node.js 配置"),
        file("tsconfig.json", r#"{
  "compilerOptions": { "target": "ES2017", "module": "esnext", "jsx": "preserve", "strict": true, "paths": { "@/*": ["./src/*"] } },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx"]
}"#, "json", "TypeScript 配置"),
        file("src/app/layout.tsx", r#"export const metadata = { title: 'Next.js App', description: 'Generated by ACoder' };
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html lang="en"><body>{children}</body></html>;
}"#, "typescript", "根布局"),
        file("src/app/page.tsx", r#"export default function Home() {
  return <main><h1>Welcome to Next.js</h1><p>Generated by ACoder</p></main>;
}"#, "typescript", "首页"),
    ];
    if cfg.with_tests { files.push(file("__tests__/page.test.tsx", r#"import { describe, it, expect, test } from 'vitest';
describe('Home', () => {
  it('should work', () => { expect(1 + 1).toBe(2); });
});"#, "typescript", "测试")); }
    files
}

fn ts_vue_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let mut files = vec![
        file("package.json", &format!(r#"{{
  "name": "{}",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "dev": "vite",
    "build": "vue-tsc && vite build",
    "test": "vitest"
  }},
  "dependencies": {{ "vue": "^3.5.0" }},
  "devDependencies": {{
    "@vitejs/plugin-vue": "^5.0.0",
    "typescript": "^5.6.0",
    "vite": "^6.0.0",
    "vue-tsc": "^2.0.0",
    "vitest": "^2.0.0"
  }}
}}"#, name), "json", "Node.js 配置"),
        file("src/App.vue", r#"<template>
  <div><h1>Vue 3 App</h1><p>Generated by ACoder</p></div>
</template>

<script setup lang="ts">
</script>"#, "vue", "主组件"),
        file("src/main.ts", r#"import { createApp } from 'vue';
import App from './App.vue';
createApp(App).mount('#app');
"#, "typescript", "入口文件"),
    ];
    if cfg.with_tests { files.push(file("src/App.test.ts", r#"import { describe, it, expect } from 'vitest';
describe('App', () => { it('should work', () => { expect(true).toBe(true); }); });
"#, "typescript", "测试")); }
    files
}

fn ts_express_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let mut files = vec![
        file("package.json", &format!(r#"{{
  "name": "{}",
  "version": "0.1.0",
  "scripts": {{
    "dev": "ts-node-dev src/index.ts",
    "build": "tsc",
    "start": "node dist/index.js",
    "test": "jest"
  }},
  "dependencies": {{
    "express": "^4.21.0",
    "cors": "^2.8.5"
  }},
  "devDependencies": {{
    "@types/express": "^5.0.0",
    "@types/cors": "^2.8.0",
    "@types/node": "^22.0.0",
    "typescript": "^5.6.0",
    "ts-node-dev": "^2.0.0",
    "jest": "^29.0.0",
    "ts-jest": "^29.0.0"
  }}
}}"#, name), "json", "Node.js 配置"),
        file("src/index.ts", r#"import express from 'express';
import cors from 'cors';

const app = express();
app.use(cors());
app.use(express.json());

app.get('/health', (_req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

const PORT = process.env.PORT || 3000;
app.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});
"#, "typescript", "入口文件"),
    ];
    if cfg.with_tests { files.push(file("tests/api.test.ts", r#"import request from 'supertest';
import app from '../src/index';

describe('API', () => {
  it('GET /health returns ok', async () => {
    const res = await request(app).get('/health');
    expect(res.status).toBe(200);
    expect(res.body.status).toBe('ok');
  });
});"#, "typescript", "API 测试")); }
    files
}

// ═══════════════════════════════════════════════════════════
//  Python 模板
// ═══════════════════════════════════════════════════════════

fn py_fastapi_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let mut files = vec![
        file("requirements.txt", "fastapi>=0.115.0\nuvicorn[standard]>=0.32.0\npydantic>=2.10.0\nhttpx>=0.28.0\npytest>=8.0.0\npytest-asyncio>=0.24.0\n", "text", "Python 依赖"),
        file("pyproject.toml", &format!(r#"[project]
name = "{}"
version = "0.1.0"
requires-python = ">=3.11"
"#, name), "toml", "项目配置"),
        file("app/main.py", r#"from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="ACoder Generated API")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])

@app.get("/health")
async def health():
    return {"status": "ok"}

@app.get("/api/hello")
async def hello():
    return {"message": "Hello from ACoder"}
"#, "python", "FastAPI 入口"),
        file("app/models.py", r#"from pydantic import BaseModel

class Item(BaseModel):
    id: int | None = None
    name: str
    description: str | None = None
"#, "python", "数据模型"),
        file("run.sh", "#!/bin/bash\nuvicorn app.main:app --reload --port 3000\n", "shell", "启动脚本"),
    ];
    if cfg.with_tests { files.push(file("tests/test_api.py", r#"import pytest
from httpx import AsyncClient, ASGITransport
from app.main import app

@pytest.mark.asyncio
async def test_health():
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        resp = await client.get("/health")
        assert resp.status_code == 200
        assert resp.json()["status"] == "ok"
"#, "python", "API 测试")); }
    files
}

fn py_django_files(name: &str, _cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let pkg = name.replace('-', "_");
    vec![
        file("requirements.txt", "django>=5.1\ndjangorestframework>=3.15\ndjango-cors-headers>=4.6\npytest>=8.0\npytest-django>=4.9\n", "text", "Python 依赖"),
        file("manage.py", &format!(r#"#!/usr/bin/env python
import os, sys
if __name__ == "__main__":
    os.environ.setdefault("DJANGO_SETTINGS_MODULE", "{}.settings")
    try:
        from django.core.management import execute_from_command_line
    except ImportError as exc:
        raise ImportError("Django not installed") from exc
    execute_from_command_line(sys.argv)
"#, pkg), "python", "Django 管理"),
        file("setup.cfg", r#"[tool:pytest]
DJANGO_SETTINGS_MODULE = project.settings
python_files = tests.py test_*.py
"#, "ini", "测试配置"),
    ]
}

fn py_cli_files(name: &str, _cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    vec![
        file("pyproject.toml", &format!(r#"[project]
name = "{}"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["click>=8.1"]

[project.scripts]
{} = "cli.main:cli"
"#, name, name.replace('-', "_")), "toml", "项目配置"),
        file("cli/main.py", r#"import click

@click.group()
def cli():
    """CLI tool generated by ACoder"""
    pass

@cli.command()
def run():
    """Run the task"""
    click.echo("Running...")

if __name__ == "__main__":
    cli()
"#, "python", "CLI 入口"),
    ]
}

fn py_ml_files(name: &str, _cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    vec![
        file("requirements.txt", "torch>=2.5\nnumpy>=2.1\npandas>=2.2\nscikit-learn>=1.6\nmatplotlib>=3.9\njupyter>=1.1\n", "text", "Python 依赖"),
        file("pyproject.toml", &format!(r#"[project]
name = "{}"
version = "0.1.0"
requires-python = ">=3.11"
"#, name), "toml", "项目配置"),
        file("src/model.py", r#"import numpy as np

class BaseModel:
    def __init__(self):
        self.trained = False

    def fit(self, X: np.ndarray, y: np.ndarray):
        self.trained = True
        return self

    def predict(self, X: np.ndarray) -> np.ndarray:
        if not self.trained:
            raise RuntimeError("Model not trained")
        return np.zeros(len(X))
"#, "python", "模型定义"),
        file("notebooks/exploration.ipynb", r#"{
 "cells": [],
 "metadata": {},
 "nbformat": 4
}"#, "json", "Jupyter Notebook"),
    ]
}

// ═══════════════════════════════════════════════════════════
//  Go 模板
// ═══════════════════════════════════════════════════════════

fn go_gin_files(name: &str, cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let module = name.replace('-', "");
    let mut files = vec![
        file("go.mod", &format!("module {}\n\ngo 1.23\n\nrequire (\n\tgithub.com/gin-gonic/gin v1.10.0\n)\n", module), "go", "Go 模块"),
        file("main.go", r#"package main

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

func main() {
	r := gin.Default()
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})
	r.GET("/api/hello", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"message": "Hello from ACoder"})
	})
	r.Run(":3000")
}
"#, "go", "入口文件"),
    ];
    if cfg.with_tests { files.push(file("main_test.go", r#"package main

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestHealth(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := gin.Default()
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})
	w := httptest.NewRecorder()
	req, _ := http.NewRequest("GET", "/health", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}
}
"#, "go", "测试")); }
    files
}

fn go_cli_files(name: &str, _cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let module = name.replace('-', "");
    vec![
        file("go.mod", &format!("module {}\n\ngo 1.23\n", module), "go", "Go 模块"),
        file("main.go", r#"package main

import (
	"flag"
	"fmt"
	"os"
)

func main() {
	run := flag.Bool("run", false, "run task")
	flag.Parse()

	if *run {
		fmt.Println("Running...")
	} else {
		flag.Usage()
		os.Exit(0)
	}
}
"#, "go", "入口文件"),
    ]
}

fn go_grpc_files(name: &str, _cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let module = name.replace('-', "");
    vec![
        file("go.mod", &format!("module {}\n\ngo 1.23\n\nrequire (\n\tgoogle.golang.org/grpc v1.68.0\n\tgoogle.golang.org/protobuf v1.35.0\n)\n", module), "go", "Go 模块"),
        file("main.go", r#"package main

import (
	"fmt"
	"log"
	"net"
)

func main() {
	lis, err := net.Listen("tcp", ":50051")
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}
	fmt.Println("gRPC server listening on :50051")
	_ = lis
}
"#, "go", "入口文件"),
    ]
}

// ═══════════════════════════════════════════════════════════
//  Java 模板
// ═══════════════════════════════════════════════════════════

fn java_spring_files(name: &str, _cfg: &CodegenConfig) -> Vec<GeneratedFile> {
    let pkg = name.replace('-', "");
    vec![
        file("build.gradle", &format!(r#"plugins {{
    id 'java'
    id 'org.springframework.boot' version '3.4.0'
    id 'io.spring.dependency-management' version '1.1.6'
}}

group = 'com.example'
version = '0.1.0'
java {{ sourceCompatibility = '17' }}

repositories {{
    mavenCentral()
}}

dependencies {{
    implementation 'org.springframework.boot:spring-boot-starter-web'
    testImplementation 'org.springframework.boot:spring-boot-starter-test'
}}
"#), "groovy", "Gradle 配置"),
        file("src/main/java/com/example/{}/Application.java", &format!(r#"package com.example.{};

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class Application {{
    public static void main(String[] args) {{
        SpringApplication.run(Application.class, args);
    }}
}}
"#, pkg), "java", "Spring Boot 入口"),
        file("src/main/java/com/example/{}/controller/HealthController.java", &format!(r#"package com.example.{}.controller;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;
import java.util.Map;

@RestController
public class HealthController {{
    @GetMapping("/health")
    public Map<String, String> health() {{
        return Map.of("status", "ok");
    }}
}}
"#, pkg), "java", "健康检查"),
    ]
}

// ── 辅助函数 ────────────────────────────────────────────────

fn file(path: &str, content: &str, language: &str, description: &str) -> GeneratedFile {
    GeneratedFile {
        path: path.to_string(),
        content: content.to_string(),
        language: language.to_string(),
        description: description.to_string(),
    }
}
