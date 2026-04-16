//! Acode — 全流程自主编码引擎
//!
//! 用法:
//!   acode run "帮我写一个 Rust HTTP 服务"
//!   acode server --port 8080

#![allow(dead_code)]
#![allow(unused_imports)]

extern crate acode;

use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use acode::Config;
use acode::sprint::{SprintRunner, Phase};
use acode::error::{Error, Result};
use acode::skill::{SkillManager, SkillCommands};
use acode::execution::engine::EngineInstance;
use acode::session::Repl;
use acode::code_understanding::Analyzer;
use acode::scaffold;

#[derive(Parser, Debug)]
#[command(name = "acode", author = "Acode Team", version, about = "Acode — 全流程自主编码引擎", long_about = None)]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,
    #[arg(short, long, global = true)]
    quiet: bool,
    #[arg(short, long, global = true, default_value = ".")]
    workdir: String,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "run", alias = "r")]
    Run {
        #[arg(required = true, last = true)]
        task: String,
        #[arg(short, long, default_value = "auto")]
        model: String,
    },
    #[command(name = "server", alias = "s")]
    Server {
        #[arg(short, long, default_value = "8080")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    #[command(name = "repl", alias = "i")]
    Repl,
    #[command(name = "ui")]
    Tui,
    #[command(name = "analyze", alias = "a")]
    Analyze {
        #[arg(default_value = ".")]
        path: String,
        #[arg(short, long, default_value = "medium")]
        depth: String,
    },
    #[command(name = "sprint", alias = "sp")]
    Sprint {
        phase: Phase,
        task: Option<String>,
    },
    #[command(name = "skill")]
    Skill {
        #[arg()]
        sub: String,
    },
    Init {
        name: String,
        #[arg(short, long, default_value = "default")]
        template: String,
    },
    #[command(name = "demo")]
    Demo {
        #[arg(value_enum, default_value = "fullstack")]
        template: String,
        #[arg(short, long, default_value = ".")]
        path: String,
    },
    Version,
}

fn setup_logging(quiet: bool, verbose: bool, workdir: &str) -> anyhow::Result<()> {
    let log_dir = std::path::Path::new(workdir).join(".acode").join("logs");
    std::fs::create_dir_all(&log_dir).context("创建日志目录失败")?;

    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "acode.log");

    let filter = if verbose { EnvFilter::new("debug") }
        else if quiet { EnvFilter::new("warn") }
        else { EnvFilter::new("info") };

    let subscriber = tracing_subscriber::registry()
        .with(fmt::layer().with_writer(file_appender).with_ansi(false))
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(true).with_filter(filter));

    tracing::subscriber::set_global_default(subscriber).context("初始化日志失败")?;
    Ok(())
}


fn parse_skill_sub(s: &str) -> anyhow::Result<SkillCommands> {
    match s {
        "list" | "List" => Ok(SkillCommands::List {}),
        s if s.starts_with("run") => Ok(SkillCommands::Run {
            name: s.trim_start_matches("run ").trim().to_string(),
            params: vec![],
        }),
        s if s.starts_with("install") => Ok(SkillCommands::Install {
            name: s.trim_start_matches("install ").trim().to_string(),
            no_confirm: false,
        }),
        s if s.starts_with("scan") => Ok(SkillCommands::Scan {
            path: s.trim_start_matches("scan ").trim().to_string(),
            use_behavioral: false,
            use_llm: false,
        }),
        s if s.starts_with("publish") => Ok(SkillCommands::Publish {
            path: s.trim_start_matches("publish ").trim().to_string(),
        }),
        s if s.starts_with("score") => Ok(SkillCommands::Score {
            name: s.trim_start_matches("score ").trim().to_string(),
        }),
        s if s.starts_with("evolve") => Ok(SkillCommands::Evolve {
            name: s.trim_start_matches("evolve ").trim().to_string(),
        }),
        _ => anyhow::bail!("未知 skill 子命令: {}", s),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    setup_logging(cli.quiet, cli.verbose, &cli.workdir)?;
    // 自动加载 .env 文件
    dotenvy::dotenv().ok();

    tracing::info!("Acode 启动 | {}", env!("CARGO_PKG_VERSION"));

    let config = Config::load()?;

    match cli.command {
        Some(Commands::Run { task, .. }) => {
            let engine = EngineInstance::new(config, cli.workdir.into()).await?;
            let result = engine.run(&task).await?;
            println!("\n{}", result.summary);
            if !result.artifacts.is_empty() {
                println!("\n📦 产物:");
                for a in &result.artifacts {
                    println!("  • {}", a.path);
                }
            }
        }
        Some(Commands::Server { host, port }) => {
            use acode::gateway::server;
            let addr: std::net::SocketAddr = format!("{}:{}", host, port).parse()?;
            let listener = tokio::net::TcpListener::bind(addr).await?;
            server::run(listener, config, cli.workdir.into()).await?;
        }
        Some(Commands::Repl) => {
            Repl::new(cli.workdir.into(), config).run().await?;
        }
        Some(Commands::Tui) => {
            #[cfg(feature = "ratatui")]
            { eprintln!("TUI: run_tui 尚未实现，请使用 --features full"); }
            #[cfg(not(feature = "ratatui"))]
            { eprintln!("TUI 需要 --features full"); }
        }
        Some(Commands::Analyze { path, depth }) => {
            let report = Analyzer::new(&path)?.analyze(depth.into()).await?;
            println!("{}", report);
        }
        Some(Commands::Sprint { phase, task }) => {
            SprintRunner::new(cli.workdir.into(), config).run_phase(phase, task.as_deref()).await?;
        }
        Some(Commands::Demo { template, path }) => {
            let output_dir = if path == "." { std::path::PathBuf::from(&cli.workdir) } else { std::path::PathBuf::from(path) };
            scaffold::create_project(&output_dir, &template).await?;
        }
        Some(Commands::Skill { sub }) => {
            let data_dir = std::path::Path::new(&cli.workdir).join(".acode").join("skills");
            std::fs::create_dir_all(&data_dir).ok();
            let mut manager = SkillManager::new(data_dir)?;
            let sub_cmd: SkillCommands = parse_skill_sub(&sub)?;
            match sub_cmd {
                SkillCommands::List {} => {
                    for s in manager.list_skills().await? {
                        println!("{} — {} (v{})", s.id, s.name, s.version);
                    }
                }
                SkillCommands::Run { name, params } => { manager.run_skill(&name, params).await?; }
                SkillCommands::Evolve { name } => { manager.evolve(&name).await?; }
                SkillCommands::Install { name, no_confirm } => {
                    if let Err(e) = manager.install(&name, no_confirm).await {
                        eprintln!("安装失败: {}", e);
                    }
                }
                SkillCommands::Scan { path, use_behavioral, use_llm } => {
                    match manager.scan_skill(&path).await {
                        Ok((result, can_proceed)) => {
                            println!("🔍 扫描结果: {} / 100 ({:?})", result.score, result.risk_level);
                            println!("发现 {} 个问题:", result.findings.len());
                            for f in &result.findings {
                                println!("  [{}] {}: {} @ {}", format!("{:?}", f.severity), f.category.name(), f.name, f.location);
                            }
                            if !result.recommendations.is_empty() {
                                println!("建议:");
                                for r in &result.recommendations { println!("  - {}", r); }
                            }
                            println!("可继续: {}", if can_proceed { "✅" } else { "❌" });
                        }
                        Err(e) => eprintln!("扫描失败: {}", e),
                    }
                    let _ = use_behavioral; let _ = use_llm;
                }
                SkillCommands::Publish { path } => {
                    match manager.publish(&path).await?.status {
                        acode::skill::PublishStatus::Approved => {
                            println!("✅ Skill 已发布: {}", path);
                        }
                        acode::skill::PublishStatus::Warning => {
                            println!("⚠️  Skill 已发布（有警告）: {}", path);
                        }
                        acode::skill::PublishStatus::Rejected => {
                            eprintln!("🚫 Skill 发布被拒绝: {}", path);
                        }
                    }
                }
                SkillCommands::Score { name } => {
                    println!("📊 Skill 安全评分: {}", name);
                }
            }
        }
        Some(Commands::Init { name, template }) => {
            let path = std::path::Path::new(&cli.workdir).join(&name);
            scaffold::create_project(&path, &template).await?;
            println!("✅ 项目已创建: {}", path.display());
        }
        Some(Commands::Version) | None => {
            println!("Acode {}", env!("CARGO_PKG_VERSION"));
        }
    }
    Ok(())
}
