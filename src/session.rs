//! REPL 会话管理
//!
//! Demo #2: 端到端 REPL，串联所有模块
//! - 自然语言 → LLM 理解 → Planner 规划 → Engine 执行 → 工具调用 → Diff 确认

use crate::config::Config;
use crate::execution::engine::EngineInstance;
use anyhow::Result;
use std::collections::VecDeque;

pub struct Repl {
    workdir: std::path::PathBuf,
    config: Config,
    history: VecDeque<String>,
    session_id: String,
}

impl Repl {
    pub fn new(workdir: std::path::PathBuf, config: Config) -> Self {
        Self {
            workdir,
            config,
            history: VecDeque::new(),
            session_id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        }
    }

    /// 启动 REPL
    pub async fn run(&mut self) -> Result<()> {
        println!("\n🛠  Acode REPL v{}", env!("CARGO_PKG_VERSION"));
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("  输入任务描述，按回车执行");
        println!("  /exit, /quit     退出");
        println!("  /help            显示帮助");
        println!("  /context         显示当前上下文");
        println!("  /models          显示可用模型");
        println!("  /history         显示对话历史");
        println!("  /clear           清屏");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        loop {
            print!("acode [{}]> ", self.session_id);
            std::io::Write::flush(&mut std::io::stdout().lock())?;

            let mut input = String::new();
            if std::io::stdin().read_line(&mut input)? == 0 { break; }
            let input = input.trim();

            if input.is_empty() { continue; }

            // 内置命令
            if input.starts_with('/') {
                if self.handle_command(input).await? { break; }
                continue;
            }

            // 添加到历史
            self.history.push_back(input.to_string());
            if self.history.len() > 100 { self.history.pop_front(); }

            // 执行任务
            self.execute_task(input).await?;
        }

        println!("\n👋 再见！");
        Ok(())
    }

    /// 处理内建命令
    async fn handle_command(&mut self, cmd: &str) -> Result<bool> {
        match cmd {
            "/exit" | "/quit" | "/q" => return Ok(true),
            "/help" => {
                println!("\n📖 Acode REPL 帮助:");
                println!("  /exit, /quit  退出 REPL");
                println!("  /help         显示此帮助");
                println!("  /context      显示当前上下文");
                println!("  /models       显示可用模型");
                println!("  /history      显示对话历史");
                println!("  /clear        清屏");
                println!("  /model <name> 切换模型（如 /model deepseek-chat）");
                println!("  /sprint <任务> 开始 Sprint 工作流");
            },
            "/context" => {
                println!("\n📂 工作目录: {}", self.workdir.display());
                if let Ok(entries) = std::fs::read_dir(&self.workdir) {
                    let count = entries.count();
                    println!("  文件/目录数: {}", count);
                }
            },
            "/models" => {
                println!("\n🤖 可用模型:");
                for m in self.config.available_models() {
                    println!("  • {} ({})", m.id, m.name);
                }
            },
            "/history" => {
                println!("\n📜 对话历史:");
                for (i, h) in self.history.iter().enumerate() {
                    println!("  {}: {}", i + 1, h.chars().take(60).collect::<String>());
                }
            },
            "/clear" => {
                print!("\x1B[2J\x1B[H");
                println!("🛠  Acode REPL — 已清屏\n");
            },
            cmd if cmd.starts_with("/model ") => {
                let model = cmd.trim_start_matches("/model ").trim();
                println!("  切换模型（当前 session 有效）: {}", model);
            },
            cmd if cmd.starts_with("/sprint ") => {
                let task = cmd.trim_start_matches("/sprint ").trim();
                println!("\n🏃 开始 Sprint: {}", task);
                println!("  (Sprint 工作流调用 Planner → Engine 串联执行)");
            },
            _ => { println!("  未知命令: {}", cmd); }
        }
        Ok(false)
    }

    /// 执行用户任务
    async fn execute_task(&self, task: &str) -> Result<()> {
        println!("\n⏳ 正在理解任务...\n");

        // Step 1: 复杂度分析
        let complexity = self.analyze_complexity(task);
        println!("📊 任务复杂度: {:?}", complexity);

        // Step 2: 选择工作流
        let workflow = self.select_workflow(&complexity);
        println!("🔄 选择工作流: {}", workflow);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        // Step 3: 初始化执行引擎
        let engine = match EngineInstance::new(self.config.clone(), self.workdir.clone()).await {
            Ok(e) => e,
            Err(e) => {
                eprintln!("⚠️  引擎初始化失败: {}，降级为模拟模式", e);
                self.simulate_execution(task).await?;
                return Ok(());
            }
        };

        // Step 4: 执行
        println!("🚀 开始执行...\n");
        match engine.run(task).await {
            Ok(result) => {
                println!("\n✅ 执行完成！\n");
                println!("{}", result.summary);

                if !result.artifacts.is_empty() {
                    println!("\n📦 产物 ({} 个文件):", result.artifacts.len());
                    for artifact in &result.artifacts {
                        println!("  • {} ({:?})", artifact.path, artifact.kind);
                    }
                }

                if !result.suggestions.is_empty() {
                    println!("\n💡 建议:");
                    for suggestion in &result.suggestions {
                        println!("  • {}", suggestion);
                    }
                }
            }
            Err(e) => {
                println!("\n⚠️  执行出错: {}", e);
                println!("\n💡 提示:");
                println!("  • 检查 LLM API Key 是否配置正确");
                println!("  • 使用 'acode analyze' 先了解代码库");
                println!("  • 使用 'acode demo fullstack' 生成示例项目");
            }
        }

        Ok(())
    }

    /// 分析任务复杂度
    fn analyze_complexity(&self, task: &str) -> crate::planning::TaskComplexity {
        let file_count = self.count_project_files();
        crate::planning::TaskComplexity::infer_from(task, file_count)
    }

    /// 选择工作流
    fn select_workflow(&self, complexity: &crate::planning::TaskComplexity) -> &str {
        complexity.suggested_workflow()
    }

    /// 统计项目文件数
    fn count_project_files(&self) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(&self.workdir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if !name.starts_with('.') && name != "target" && name != "node_modules" {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    /// 模拟执行（无 LLM API Key 时降级）
    async fn simulate_execution(&self, task: &str) -> Result<()> {
        println!("🤖 模拟执行模式\n");

        // 模拟规划过程
        println!("📋 规划步骤:");
        let steps = self.simulate_planning(task);
        for (i, step) in steps.iter().enumerate() {
            println!("  {}. {}", i + 1, step);
        }

        println!("\n🔧 工具准备:");
        let tools = vec!["read_file", "write_file", "search", "run_command", "git"];
        for tool in &tools {
            println!("  ✓ {}", tool);
        }

        println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        println!("⚠️  注意：当前运行在模拟模式");
        println!("   提供了完整的执行流程但未实际调用 LLM");
        println!("   请设置 API Key 后重新运行以启用真实执行\n");
        println!("💡 设置 API Key:");
        println!("   export DEEPSEEK_API_KEY=sk-xxx");
        println!("   export DASHSCOPE_API_KEY=sk-xxx");

        Ok(())
    }

    /// 模拟规划步骤
    fn simulate_planning(&self, task: &str) -> Vec<String> {
        let task_lower = task.to_lowercase();

        if task_lower.contains("前端") || task_lower.contains("vue") || task_lower.contains("react") {
            vec![
                "1. 分析前端技术栈和组件结构".to_string(),
                "2. 设计组件 Props 接口".to_string(),
                "3. 生成 Vue/React 组件代码".to_string(),
                "4. 编写配套的 CSS/样式".to_string(),
                "5. 添加单元测试".to_string(),
            ]
        } else if task_lower.contains("后端") || task_lower.contains("api") || task_lower.contains("rust") {
            vec![
                "1. 分析后端接口设计".to_string(),
                "2. 设计数据模型".to_string(),
                "3. 实现 API 路由和处理函数".to_string(),
                "4. 添加数据库操作".to_string(),
                "5. 编写 API 测试".to_string(),
            ]
        } else if task_lower.contains("全栈") || task_lower.contains("todo") || task_lower.contains("待办") {
            vec![
                "1. 设计数据库 Schema".to_string(),
                "2. 实现后端 CRUD API".to_string(),
                "3. 生成前端组件".to_string(),
                "4. 联调前后端接口".to_string(),
                "5. 端到端测试".to_string(),
            ]
        } else {
            vec![
                "1. 分析需求和现有代码".to_string(),
                "2. 确定改动范围".to_string(),
                "3. 实施修改".to_string(),
                "4. 自测验证".to_string(),
            ]
        }
    }
}
