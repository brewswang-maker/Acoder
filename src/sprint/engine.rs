//! Sprint 工作流引擎
//!
//! 7 阶段完整开发闭环：Think → Plan → Build → Review → Test → Ship → Reflect

use std::path::PathBuf;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::Result;
use crate::llm::{Client as LlmClient, Message, LlmRequest};
use crate::codegen::{CodegenEngine, CodegenConfig, CodeStyle};
use crate::codegen::detector::LanguageDetector;

// ── 数据结构 ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
pub enum SprintPhase {
    Think,
    Plan,
    Build,
    Review,
    Test,
    Ship,
    Reflect,
}

impl SprintPhase {
    pub fn all() -> &'static [SprintPhase] {
        &[SprintPhase::Think, SprintPhase::Plan, SprintPhase::Build,
          SprintPhase::Review, SprintPhase::Test, SprintPhase::Ship, SprintPhase::Reflect]
    }

    pub fn description(&self) -> &'static str {
        match self {
            SprintPhase::Think  => "想清楚再动手",
            SprintPhase::Plan   => "生成执行计划",
            SprintPhase::Build  => "开始编码",
            SprintPhase::Review => "代码审查",
            SprintPhase::Test   => "测试验证",
            SprintPhase::Ship   => "发布上线",
            SprintPhase::Reflect=> "复盘总结",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    pub phase: SprintPhase,
    pub status: PhaseStatus,
    pub duration_ms: u64,
    pub output: String,
    pub artifacts: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
pub enum PhaseStatus { Pending, Running, Success, Failed, Skipped }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintResult {
    pub task: String,
    pub phase_results: Vec<PhaseResult>,
    pub total_duration_ms: u64,
    pub artifacts: Vec<String>,
    pub summary: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

// ── Sprint Engine ────────────────────────────────────────────

pub struct SprintEngine {
    workdir: PathBuf,
    config: Arc<Config>,
    llm: LlmClient,
}

impl SprintEngine {
    pub async fn new(workdir: PathBuf, config: Config) -> Result<Self> {
        let config = Arc::new(config);
        let llm = LlmClient::new(Arc::clone(&config).llm.clone());
        Ok(Self { workdir, config, llm })
    }

    /// 执行完整 Sprint（7 阶段）
    pub async fn run_full_sprint(&self, task: &str) -> Result<SprintResult> {
        let started_at = Utc::now();

        println!("\n=== ACoder Sprint Engine ===");
        println!("任务: {}", task);

        let mut phase_results = Vec::new();
        let mut all_artifacts = Vec::new();
        let mut total_ms = 0u64;

        for phase in SprintPhase::all() {
            let start = std::time::Instant::now();
            let res = self.run_phase(*phase, task).await;
            let dur = start.elapsed().as_millis() as u64;
            total_ms += dur;

            let pr = match res {
                Ok((output, artifacts, warnings)) => {
                    PhaseResult { phase: *phase, status: PhaseStatus::Success,
                        duration_ms: dur, output: output.clone(),
                        artifacts: artifacts.clone(), warnings: warnings.clone() }
                }
                Err(e) => {
                    let msg = format!("失败: {}", e);
                    PhaseResult { phase: *phase, status: PhaseStatus::Failed,
                        duration_ms: dur, output: msg.clone(),
                        artifacts: Vec::new(), warnings: vec![e.to_string()] }
                }
            };

            println!("[{}] {} ({})", pr.status, phase, dur);
            all_artifacts.extend(pr.artifacts.clone());
            phase_results.push(pr);
        }

        let ended_at = Utc::now();
        let ok_count = phase_results.iter().filter(|r| r.status == PhaseStatus::Success).count();
        let summary = format!("完成: {}/{} | {}ms", ok_count, phase_results.len(), total_ms);

        Ok(SprintResult {
            task: task.to_string(),
            phase_results,
            total_duration_ms: total_ms,
            artifacts: all_artifacts,
            summary,
            started_at,
            ended_at,
        })
    }

    /// 执行单个阶段
    pub async fn run_phase(&self, phase: SprintPhase, task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        match phase {
            SprintPhase::Think  => self.phase_think(task).await,
            SprintPhase::Plan   => self.phase_plan(task).await,
            SprintPhase::Build => self.phase_build(task).await,
            SprintPhase::Review=> self.phase_review(task).await,
            SprintPhase::Test  => self.phase_test(task).await,
            SprintPhase::Ship   => self.phase_ship(task).await,
            SprintPhase::Reflect=> self.phase_reflect(task).await,
        }
    }

    // ── Think: 分析需求 ─────────────────────────────────────
    async fn phase_think(&self, task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        let detection = LanguageDetector::detect(task);

        let prompt = format!(
            "你是架构师。请分析任务: {}\n\n检测到的技术栈: {} {:?}\n\n请回答：\n1. 项目类型和技术方案\n2. 关键风险\n3. 建议的组件架构",
            task, detection.primary, detection.framework
        );

        let req = LlmRequest {
            model: "auto".into(), messages: vec![Message::user(&prompt)],
            temperature: Some(0.3), max_tokens: Some(1200), stream: false, tools: None,
        };

        let resp = self.llm.complete(req).await?;
        let summary = format!("技术栈: {} | 框架: {:?}\n\n{}", detection.primary, detection.framework, resp.content);
        Ok((summary, Vec::new(), Vec::new()))
    }

    // ── Plan: 生成计划 ──────────────────────────────────────
    async fn phase_plan(&self, task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        let prompt = format!(
            "请将以下任务拆解为具体执行步骤:\n{}\n\n每行格式: [步骤N] [复杂度] 描述", task
        );

        let req = LlmRequest {
            model: "auto".into(), messages: vec![Message::user(&prompt)],
            temperature: Some(0.2), max_tokens: Some(600), stream: false, tools: None,
        };

        let resp = self.llm.complete(req).await?;
        let plan_file = self.workdir.join("plan.md");
        let content = format!("# 执行计划\n\n任务: {}\n\n{}", task, resp.content);
        tokio::fs::write(&plan_file, &content).await.ok();
        Ok((resp.content.clone(), vec![plan_file.to_string_lossy().to_string()], Vec::new()))
    }

    // ── Build: 编码 ────────────────────────────────────────
    async fn phase_build(&self, task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        let detection = LanguageDetector::detect(task);
        let output_dir = self.workdir.join("output");
        tokio::fs::create_dir_all(&output_dir).await.ok();

        let codegen = CodegenEngine::with_llm(self.llm.clone());
        let config = CodegenConfig {
            language: detection.primary.clone(),
            framework: detection.framework.clone(),
            project_name: "generated".to_string(),
            with_tests: true,
            with_ci: false,
            with_docker: false,
            style: CodeStyle::Standard,
        };

        let files = codegen.generate_from_description(task, output_dir.clone(), config).await?;
        let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();

        let summary = format!("生成 {} 个文件到 {}", files.len(), output_dir.display());
        Ok((summary, paths, Vec::new()))
    }

    // ── Review: 审查 ───────────────────────────────────────
    async fn phase_review(&self, _task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        let output_dir = self.workdir.join("output");
        if !output_dir.exists() {
            return Ok(("Review 跳过: output 目录不存在".to_string(), Vec::new(), vec!["Build 未完成".to_string()]));
        }

        let mut codes = Vec::new();
        let mut entries = tokio::fs::read_dir(&output_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ["rs", "ts", "tsx", "js", "py", "go", "java"].contains(&ext) {
                    if let Ok(c) = tokio::fs::read_to_string(&path).await {
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        codes.push(format!("// ===== {} =====\n{}", name, &c[..c.len().min(1500)]));
                    }
                }
            }
        }

        if codes.is_empty() {
            return Ok(("无代码文件可审查".to_string(), Vec::new(), Vec::new()));
        }

        let prompt = format!("审查以下代码，检测安全/错误处理/风格问题:\n{}\n\n只返回发现的问题列表，没有问题则说\"审查通过\"", codes.join("\n\n"));
        let req = LlmRequest {
            model: "auto".into(), messages: vec![Message::user(&prompt)],
            temperature: Some(0.1), max_tokens: Some(800), stream: false, tools: None,
        };
        let resp = self.llm.complete(req).await?;
        let warns = if resp.content.contains("通过") { Vec::new() } else { vec!["Review 发现问题".to_string()] };
        Ok((resp.content, Vec::new(), warns))
    }

    // ── Test: 测试 ──────────────────────────────────────────
    async fn phase_test(&self, _task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        let output_dir = self.workdir.join("output");
        let test_dir = output_dir.join("tests");
        tokio::fs::create_dir_all(&test_dir).await.ok();

        let prompt = "为前面的任务生成测试用例要点（注释形式），包含：1.基础测试 2.边界条件 3.错误处理";
        let req = LlmRequest {
            model: "auto".into(), messages: vec![Message::user(prompt)],
            temperature: Some(0.1), max_tokens: Some(600), stream: false, tools: None,
        };
        let resp = self.llm.complete(req).await?;
        let test_file = test_dir.join("sprint_tests.md");
        tokio::fs::write(&test_file, &resp.content).await.ok();

        Ok((format!("测试要点已写入 {}", test_file.display()), vec![test_file.to_string_lossy().to_string()], Vec::new()))
    }

    // ── Ship: 发布 ─────────────────────────────────────────
    async fn phase_ship(&self, task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        let detection = LanguageDetector::detect(task);
        let output_dir = self.workdir.join("output");
        tokio::fs::create_dir_all(&output_dir).await.ok();

        let dockerfile = match detection.primary.as_str() {
            "rust"    => "FROM rust:1.75-slim\nWORKDIR /app\nCOPY Cargo.toml Cargo.lock ./\nCOPY src ./src\nRUN cargo build --release\nEXPOSE 8080\nCMD [\"target/release/generated\"]",
            "typescript" | "javascript" => "FROM node:20-alpine\nWORKDIR /app\nCOPY package*.json ./\nRUN npm ci\nCOPY . .\nEXPOSE 3000\nCMD [\"node\", \"dist\"]",
            "python"  => "FROM python:3.11-slim\nWORKDIR /app\nCOPY requirements.txt ./\nRUN pip install -r requirements.txt\nCOPY . .\nEXPOSE 8000\nCMD [\"uvicorn\", \"src:app\", \"--host\", \"0.0.0.0\"]",
            "golang"  => "FROM golang:1.21-alpine\nWORKDIR /app\nCOPY go.mod go.sum ./\nRUN go mod download\nCOPY . .\nRUN go build -o server\nEXPOSE 8080\nCMD [\"./server\"]",
            _ => "# Unsupported language",
        };

        let df_path = output_dir.join("Dockerfile");
        tokio::fs::write(&df_path, dockerfile).await.ok();

        let ci_path = output_dir.join(".github/workflows/ci.yml");
        if let Some(p) = ci_path.parent() { tokio::fs::create_dir_all(p).await.ok(); }
        let ci = self.build_ci_content(&detection.primary);
        tokio::fs::write(&ci_path, &ci).await.ok();

        let artifacts = vec![
            df_path.to_string_lossy().to_string(),
            ci_path.to_string_lossy().to_string(),
        ];
        let summary = format!("Dockerfile + CI/CD 生成完成 | 语言: {}", detection.primary);
        Ok((summary, artifacts, Vec::new()))
    }

    fn build_ci_content(&self, lang: &str) -> String {
        let install = match lang {
            "rust" => "cargo fetch",
            "typescript" => "npm ci",
            "python" => "pip install -r requirements.txt",
            "golang" => "go mod download",
            _ => "echo ok",
        };
        let build = match lang {
            "rust" => "cargo build --release",
            "typescript" => "npm run build",
            "golang" => "go build -o server",
            _ => "echo ok",
        };
        let test = match lang {
            "rust" => "cargo test",
            "typescript" => "npm test",
            "golang" => "go test ./...",
            _ => "echo ok",
        };
        format!("name: CI\non: [push]\njobs:\n  ci:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - name: Setup {}\n        uses: ./\n      - run: {}\n      - run: {}\n      - run: {}", lang, install, build, test)
    }

    // ── Reflect: 复盘 ─────────────────────────────────────
    async fn phase_reflect(&self, task: &str) -> Result<(String, Vec<String>, Vec<String>)> {
        let now = Utc::now().format("%Y-%m-%d %H:%M").to_string();
        let reflect_path = self.workdir.join("sprint-reflect.md");

        let prompt = format!(
            "为以下 Sprint 任务写复盘报告:\n{}\n\n包含: 1.完成情况 2.经验教训 3.改进建议 4.后续计划", task
        );
        let req = LlmRequest {
            model: "auto".into(), messages: vec![Message::user(&prompt)],
            temperature: Some(0.3), max_tokens: Some(600), stream: false, tools: None,
        };
        let resp = self.llm.complete(req).await?;
        let content = format!("# Sprint 复盘 [{}]\n\n任务: {}\n\n{}", now, task, resp.content);
        tokio::fs::write(&reflect_path, &content).await.ok();

        Ok((content, vec![reflect_path.to_string_lossy().to_string()], Vec::new()))
    }
}
