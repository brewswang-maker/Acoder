//! TUI App 状态管理

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ratatui::{
    backend::CrosstermBackend,
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
    Terminal,
};

use crate::config::Config;
use super::input::InputMode;

/// 输出消息
#[derive(Debug, Clone)]
pub enum OutputMsg {
    /// Agent 思考中
    Thinking(String),
    /// Agent 正在调用工具
    ToolCall { name: String, args: String },
    /// Agent 输出文本
    Text(String),
    /// 任务完成
    Done { summary: String },
    /// 任务失败
    Error(String),
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Idle,
    Running,
    Done,
    Error,
}

/// TUI 应用状态
pub struct App {
    /// 工作目录
    pub workdir: PathBuf,
    /// 配置
    pub config: Arc<Config>,
    /// 输入模式
    pub input_mode: InputMode,
    /// 当前输入
    pub input: String,
    /// 输入历史
    pub history: VecDeque<String>,
    /// 历史索引
    pub history_idx: Option<usize>,
    /// Agent 输出缓冲
    pub outputs: VecDeque<OutputMsg>,
    /// 滚动位置
    pub scroll: u16,
    /// 任务状态
    pub task_status: TaskStatus,
    /// 开始时间
    pub started_at: Option<Instant>,
    /// 输出接收通道
    pub output_rx: tokio::sync::mpsc::Receiver<OutputMsg>,
}

impl App {
    pub fn new(workdir: PathBuf, config: Arc<Config>) -> anyhow::Result<Self> {
        let (_, rx) = tokio::sync::mpsc::channel(100);
        Ok(Self {
            workdir,
            config,
            input_mode: InputMode::Normal,
            input: String::new(),
            history: VecDeque::with_capacity(100),
            history_idx: None,
            outputs: VecDeque::with_capacity(500),
            scroll: 0,
            task_status: TaskStatus::Idle,
            started_at: None,
            output_rx: rx,
        })
    }

    /// 处理按键，返回 true 表示退出
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> anyhow::Result<bool> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('q') if self.input_mode == InputMode::Normal => {
                return Ok(true);
            }
            KeyCode::Char('e') if self.input_mode == InputMode::Normal => {
                self.input_mode = InputMode::Editing;
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Enter => {
                if self.input_mode == InputMode::Editing && !self.input.is_empty() {
                    self.submit_task()?;
                }
            }
            KeyCode::Up => {
                if self.input_mode == InputMode::Editing {
                    self.navigate_history(-1);
                }
            }
            KeyCode::Down => {
                if self.input_mode == InputMode::Editing {
                    self.navigate_history(1);
                }
            }
            KeyCode::Char(c) if self.input_mode == InputMode::Editing => {
                self.input.push(c);
            }
            KeyCode::Backspace if self.input_mode == InputMode::Editing => {
                self.input.pop();
            }
            KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                self.input.clear();
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }

        Ok(false)
    }

    /// 提交任务
    fn submit_task(&mut self) -> anyhow::Result<()> {
        let task = self.input.clone();
        self.input.clear();
        self.input_mode = InputMode::Normal;

        self.history.push_front(task.clone());
        if self.history.len() > 100 {
            self.history.pop_back();
        }

        self.task_status = TaskStatus::Running;
        self.started_at = Some(Instant::now());

        tracing::info!("任务已提交: {}", task);
        Ok(())
    }

    /// 历史导航
    fn navigate_history(&mut self, delta: isize) {
        let len = self.history.len();
        if len == 0 { return; }

        match self.history_idx {
            None => {
                self.history_idx = Some(0);
            }
            Some(idx) => {
                let new_idx = if delta > 0 {
                    if idx == 0 { len - 1 } else { idx - 1 }
                } else {
                    if idx >= len - 1 { 0 } else { idx + 1 }
                };
                self.history_idx = Some(new_idx);
            }
        }

        if let Some(idx) = self.history_idx {
            self.input = self.history[idx].clone();
        }
    }

    /// 添加 Agent 输出
    pub fn push_output(&mut self, msg: OutputMsg) {
        self.outputs.push_back(msg);
        if self.outputs.len() > 500 {
            self.outputs.pop_front();
        }
        self.scroll = u16::MAX;
    }

    /// 渲染 TUI
    pub fn render(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // 标题栏
                Constraint::Min(0),       // 输出区
                Constraint::Length(3),   // 输入栏
            ])
            .split(f.size());

        self.render_header(f, chunks[0]);
        self.render_output(f, chunks[1]);
        self.render_input(f, chunks[2]);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let title = match self.task_status {
            TaskStatus::Idle => "Acode — 空闲",
            TaskStatus::Running => "运行中",
            TaskStatus::Done => "完成",
            TaskStatus::Error => "错误",
        };

        let elapsed = self.started_at
            .map(|t| format!(" {:.1}s ", t.elapsed().as_secs_f32()))
            .unwrap_or_default();

        let line = Line::from(vec![
            Span::raw("Acode TUI | "),
            Span::raw(title),
            Span::raw(elapsed.as_str()),
            Span::raw("  [q]退出 [e]编辑 [Enter]执行"),
        ]);

        let para = Paragraph::new(vec![line])
            .block(Block::bordered().title(" Acode "));
        f.render_widget(para, area);
    }

    fn render_output(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.outputs
            .iter()
            .map(|msg| {
                let (content, fg) = match msg {
                    OutputMsg::Thinking(s) => (s.clone(), Color::Yellow),
                    OutputMsg::ToolCall { name, args } => (
                        format!("{}: {}", name, args),
                        Color::Blue,
                    ),
                    OutputMsg::Text(s) => (s.clone(), Color::White),
                    OutputMsg::Done { summary } => (
                        format!("Done: {}", summary),
                        Color::Green,
                    ),
                    OutputMsg::Error(s) => (
                        format!("Error: {}", s),
                        Color::Red,
                    ),
                };
                ListItem::new(Line::from(Span::styled(content, Style::new().fg(fg))))
            })
            .collect();

        let list = List::new(items)
            .block(Block::bordered().title(" Output "));
        f.render_widget(list, area);
    }

    fn render_input(&self, f: &mut Frame, area: Rect) {
        let cursor = if self.input_mode == InputMode::Editing { "▋" } else { " " };
        let prompt = if self.input_mode == InputMode::Editing { "> " } else { "  " };

        let line = Line::from(vec![
            Span::raw(prompt),
            Span::raw(&self.input),
            Span::raw(cursor),
        ]);

        let border_style = match self.input_mode {
            InputMode::Normal => Style::new().fg(Color::White),
            InputMode::Editing => Style::new().fg(Color::Cyan),
        };

        let para = Paragraph::new(vec![line])
            .block(
                Block::bordered()
                    .border_style(border_style)
                    .title(if self.input_mode == InputMode::Editing { " Command " } else { " [e] to edit " }),
            );
        f.render_widget(para, area);
    }
}
