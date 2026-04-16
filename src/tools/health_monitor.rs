//! 工具健康监控
use std::collections::HashMap;

pub struct HealthMonitor {
    tool_health: HashMap<String, ToolHealth>,
}

#[derive(Debug, Clone)]
pub struct ToolHealth {
    pub name: String,
    pub call_count: u64,
    pub success_count: u64,
    pub avg_latency_ms: f64,
}

impl HealthMonitor {
    pub fn new() -> Self { Self { tool_health: HashMap::new() } }
    pub fn record_call(&mut self, name: &str, success: bool, latency_ms: f64) {
        let h = self.tool_health.entry(name.into()).or_insert(ToolHealth {
            name: name.into(), call_count: 0, success_count: 0, avg_latency_ms: 0.0,
        });
        h.call_count += 1;
        if success { h.success_count += 1; }
        h.avg_latency_ms = (h.avg_latency_ms * (h.call_count - 1) as f64 + latency_ms) / h.call_count as f64;
    }
}

impl Default for HealthMonitor { fn default() -> Self { Self::new() } }
