//! TUI 输入处理

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, Event};

/// 输入模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// 普通模式
    Normal,
    /// 编辑模式
    Editing,
}

/// 获取下一个按键事件（阻塞）
pub fn next_key() -> Option<KeyEvent> {
    loop {
        if let Ok(event) = crossterm::event::read() {
            if let Event::Key(key) = event {
                if key.kind == KeyEventKind::Press {
                    return Some(key);
                }
            }
        }
    }
}
