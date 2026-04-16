//! Acode Editor — 独立 AI 代码编辑器
//!
//! 运行: cargo run --bin acode-editor -- [--workdir <path>]
//!
//! 快捷键：
//! - Ctrl+Shift+P: 命令面板
//! - Ctrl+P: 快速打开文件
//! - Ctrl+S: 保存
//! - Ctrl+W: 关闭标签
//! - Ctrl+Tab / Ctrl+Shift+Tab: 切换标签

use std::env;
use std::path::PathBuf;
use std::process::exit;

fn main() {
    // 解析参数
    let args: Vec<String> = env::args().collect();
    let mut workdir: Option<PathBuf> = None;

    for i in 1..args.len() {
        match args[i].as_str() {
            "--workdir" | "-w" => {
                if i + 1 < args.len() {
                    workdir = Some(PathBuf::from(&args[i + 1]));
                }
            }
            "--help" | "-h" => {
                println!("Acode Editor — 独立 AI 代码编辑器");
                println!();
                println!("用法: acode-editor [选项]");
                println!();
                println!("选项:");
                println!("  --workdir, -w <path>    设置工作目录");
                println!("  --help, -h              显示帮助");
                exit(0);
            }
            _ => {}
        }
    }

    // 如果没有指定 workdir，使用当前目录
    let workdir = workdir.unwrap_or_else(|| env::current_dir().unwrap_or_default());

    println!("🚀 启动 Acode Editor | workdir: {}", workdir.display());

    // 初始化 tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // 启动编辑器
    acode::editor::run(Some(workdir));
}
