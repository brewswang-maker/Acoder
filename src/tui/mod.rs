//! Acode TUI — 交互式终端界面
//!
//! 启动：`acode ui`

use std::io::{self, stdout, Write};
use std::sync::Arc;

use anyhow::Result;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::config::Config;

mod app;
mod input;
mod output;

pub use app::{App, OutputMsg, TaskStatus};
pub use input::InputMode;

// ── 入口 ───────────────────────────────────────────────────────────────

/// 运行 TUI（阻塞）
pub fn run_tui(workdir: std::path::PathBuf, config: Config) -> Result<()> {
    let mut stdout = stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    crossterm::execute!(stdout, crossterm::cursor::Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(workdir, Arc::new(config))?;
    let res = run_loop(&mut terminal, app);

    // 恢复终端（忽略错误）
    let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    let _ = crossterm::execute!(io::stdout(), crossterm::cursor::Show);

    res
}

// ── 主循环 ─────────────────────────────────────────────────────────────

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> Result<()> {
    terminal.clear()?;

    loop {
        terminal.draw(|f| app.render(f))?;

        while let Ok(msg) = app.output_rx.try_recv() {
            app.push_output(msg);
        }

        if let Some(key) = input::next_key() {
            if app.handle_key(key)? {
                return Ok(());
            }
        }
    }
}
