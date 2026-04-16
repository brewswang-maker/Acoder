//! API Module — REST API + Rate Limiting
//!
//! 设计规格: 产品设计文档 v3.0 §11.5.5, §11.5.7
//! - 滑动窗口限流: 每分钟/每小时/每天
//! - 熔断机制: 异常流量自动熔断
//! - 配额预警: 达到 80% 时通知
//! - 多维度限流: 按用户/按 API Key/按端点

pub mod routes;
pub mod rate_limiter;
pub mod handlers;
pub mod models;
pub mod middleware;
