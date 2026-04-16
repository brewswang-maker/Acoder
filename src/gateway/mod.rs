//! Gateway 模块
//!
//! HTTP/WebSocket 网关：
//! - HTTP REST API
//! - WebSocket 实时通信
//! - SSE 流式推送
//! - 认证中间件
//! - 远程控制

pub mod server;
pub mod websocket;
pub mod auth;

pub use server::run as run_gateway;
