//! Rate Limiter — Sliding Window + Circuit Breaker
//!
//! 设计规格: 产品设计文档 v3.0 §11.5.5, §11.5.7
//! - 滑动窗口算法: 每分钟/每小时/每天限制
//! - 熔断机制: 连续失败 N 次后 Open，1 分钟后 HalfOpen 试探
//! - 配额预警: 达到 80% 时在响应中通知
//! - 多维度限流: 按用户 / 按端点

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};

// ── Config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub per_minute: usize,
    pub per_hour: usize,
    pub per_day: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_minute: 60,
            per_hour: 3000,
            per_day: 50000,
        }
    }
}

// ── Sliding Window ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SlidingWindow {
    pub limit: usize,
    /// Timestamps of requests within the window
    pub requests: Vec<DateTime<Utc>>,
    /// Window duration in seconds
    pub window_secs: u64,
}

impl SlidingWindow {
    pub fn new(limit: usize, window_secs: u64) -> Self {
        Self {
            limit,
            requests: Vec::new(),
            window_secs,
        }
    }

    /// Clean up expired entries and return the count of remaining requests
    fn clean_expired(&mut self, now: DateTime<Utc>) {
        let cutoff = now - Duration::seconds(self.window_secs as i64);
        self.requests.retain(|&t| t > cutoff);
    }

    /// Current request count in window
    pub fn count(&self, now: DateTime<Utc>) -> usize {
        let cutoff = now - Duration::seconds(self.window_secs as i64);
        self.requests.iter().filter(|&&t| t > cutoff).count()
    }

    /// Add a request timestamp
    pub fn add(&mut self, now: DateTime<Utc>) {
        self.requests.push(now);
    }

    /// Remaining quota
    pub fn remaining(&self, now: DateTime<Utc>) -> usize {
        self.limit.saturating_sub(self.count(now))
    }

    /// Seconds until the oldest request expires (reset window)
    pub fn reset_in_secs(&self, now: DateTime<Utc>) -> u64 {
        if self.requests.is_empty() {
            return 0;
        }
        let oldest = *self.requests.iter().min().unwrap();
        let expires_at = oldest + Duration::seconds(self.window_secs as i64);
        let diff = (expires_at - now).num_seconds();
        diff.max(0) as u64
    }
}

// ── User Rate Limit ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UserRateLimit {
    pub user_id: String,
    pub minute_window: SlidingWindow,
    pub hour_window: SlidingWindow,
    pub daily_window: SlidingWindow,
}

impl UserRateLimit {
    pub fn new(user_id: String, config: &RateLimitConfig) -> Self {
        Self {
            user_id,
            minute_window: SlidingWindow::new(config.per_minute, 60),
            hour_window: SlidingWindow::new(config.per_hour, 3600),
            daily_window: SlidingWindow::new(config.per_day, 86400),
        }
    }
}

// ── Circuit Breaker ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    Closed,   // 正常
    Open,    // 熔断中
    HalfOpen, // 试探
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub name: String,
    pub state: CircuitState,
    pub failure_count: usize,
    pub last_failure: Option<DateTime<Utc>>,
    /// Failure threshold before opening
    pub threshold: usize,
    /// How long to stay open before half-open (seconds)
    pub reset_timeout_secs: u64,
}

impl CircuitBreaker {
    pub fn new(name: String, threshold: usize) -> Self {
        Self {
            name,
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure: None,
            threshold,
            reset_timeout_secs: 60,
        }
    }

    /// Record a failure and update state
    pub fn record_failure(&mut self, now: DateTime<Utc>) {
        self.failure_count += 1;
        self.last_failure = Some(now);

        if self.state == CircuitState::HalfOpen {
            // Failed during half-open → go back to open
            self.state = CircuitState::Open;
        } else if self.failure_count >= self.threshold {
            self.state = CircuitState::Open;
        }
    }

    /// Record a success
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
    }

    /// Check if circuit should transition Open → HalfOpen
    pub fn check_timeout(&mut self, now: DateTime<Utc>) {
        if self.state == CircuitState::Open {
            if let Some(last) = self.last_failure {
                if (now - last).num_seconds() >= self.reset_timeout_secs as i64 {
                    self.state = CircuitState::HalfOpen;
                }
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == CircuitState::Open
    }
}

// ── Rate Limit Result ───────────────────────────────────────────────────

pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: usize,
    pub limit: usize,
    pub reset_in_secs: u64,
}

impl RateLimitResult {
    pub fn limited(remaining: usize, limit: usize, reset_in_secs: u64) -> Self {
        Self {
            allowed: false,
            remaining,
            limit,
            reset_in_secs,
        }
    }

    pub fn allowed(remaining: usize, limit: usize, reset_in_secs: u64) -> Self {
        Self {
            allowed: true,
            remaining,
            limit,
            reset_in_secs,
        }
    }
}

// ── Quota Warning ───────────────────────────────────────────────────────

/// Returns true if usage exceeds 80% of limit
pub fn is_quota_warning(used: usize, limit: usize) -> bool {
    if limit == 0 {
        return false;
    }
    (used as f64 / limit as f64) >= 0.8
}

// ── Rate Limiter ────────────────────────────────────────────────────────

pub struct RateLimiter {
    user_limits: Arc<RwLock<HashMap<String, UserRateLimit>>>,
    global_limit: RateLimitConfig,
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        Self {
            user_limits: self.user_limits.clone(),
            global_limit: self.global_limit.clone(),
            circuit_breakers: self.circuit_breakers.clone(),
        }
    }
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            user_limits: Arc::new(RwLock::new(HashMap::new())),
            global_limit: config,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a request from `user_id` is allowed.
    /// Checks minute → hour → day in order.
    pub async fn check(&self, user_id: &str) -> RateLimitResult {
        let now = Utc::now();
        let mut limits = self.user_limits.write().await;

        let user_limit = limits
            .entry(user_id.to_string())
            .or_insert_with(|| UserRateLimit::new(user_id.to_string(), &self.global_limit));

        // Clean expired entries
        user_limit.minute_window.clean_expired(now);
        user_limit.hour_window.clean_expired(now);
        user_limit.daily_window.clean_expired(now);

        // Check in order: minute → hour → day
        let minute_count = user_limit.minute_window.count(now);
        if minute_count >= user_limit.minute_window.limit {
            return RateLimitResult::limited(
                0,
                user_limit.minute_window.limit,
                user_limit.minute_window.reset_in_secs(now),
            );
        }

        let hour_count = user_limit.hour_window.count(now);
        if hour_count >= user_limit.hour_window.limit {
            return RateLimitResult::limited(
                0,
                user_limit.hour_window.limit,
                user_limit.hour_window.reset_in_secs(now),
            );
        }

        let day_count = user_limit.daily_window.count(now);
        if day_count >= user_limit.daily_window.limit {
            return RateLimitResult::limited(
                0,
                user_limit.daily_window.limit,
                user_limit.daily_window.reset_in_secs(now),
            );
        }

        // All within limits — remaining is the smallest window quota
        let remaining = user_limit
            .minute_window
            .remaining(now)
            .min(user_limit.hour_window.remaining(now))
            .min(user_limit.daily_window.remaining(now));

        RateLimitResult::allowed(remaining, user_limit.minute_window.limit, 60)
    }

    /// Record a request for `user_id`
    pub async fn record(&self, user_id: &str) {
        let now = Utc::now();
        let mut limits: tokio::sync::RwLockWriteGuard<'_, _> = self.user_limits.write().await;

        let user_limit = limits
            .entry(user_id.to_string())
            .or_insert_with(|| UserRateLimit::new(user_id.to_string(), &self.global_limit));

        user_limit.minute_window.add(now);
        user_limit.hour_window.add(now);
        user_limit.daily_window.add(now);
    }

    /// Get usage stats for a user
    pub async fn get_usage(&self, user_id: &str) -> UsageStats {
        let now = Utc::now();
        let limits = self.user_limits.read().await;

        let user = limits.get(user_id);

        let (minute_used, minute_limit) = user
            .map(|u| (u.minute_window.count(now), u.minute_window.limit))
            .unwrap_or((0, self.global_limit.per_minute));

        let (hour_used, hour_limit) = user
            .map(|u| (u.hour_window.count(now), u.hour_window.limit))
            .unwrap_or((0, self.global_limit.per_hour));

        let (day_used, day_limit) = user
            .map(|u| (u.daily_window.count(now), u.daily_window.limit))
            .unwrap_or((0, self.global_limit.per_day));

        UsageStats {
            user_id: user_id.to_string(),
            period_start: now - Duration::days(1),
            period_end: now,
            total_requests: day_used,
            total_tokens: 0, // filled by billing service
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_estimate: 0.0,
            quota_limit: day_limit,
            quota_used: day_used,
            quota_remaining: day_limit.saturating_sub(day_used),
        }
    }

    /// Check circuit breaker for an endpoint
    pub async fn check_circuit(&self, endpoint: &str) -> bool {
        let mut breakers = self.circuit_breakers.write().await;
        let now = Utc::now();

        if let Some(cb) = breakers.get_mut(endpoint) {
            cb.check_timeout(now);
            !cb.is_open()
        } else {
            true
        }
    }

    /// Record an API failure for circuit breaker
    pub async fn record_failure(&self, endpoint: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        let now = Utc::now();

        let cb = breakers
            .entry(endpoint.to_string())
            .or_insert_with(|| CircuitBreaker::new(endpoint.to_string(), 5));
        cb.record_failure(now);
    }

    /// Record an API success (closes circuit breaker)
    pub async fn record_success(&self, endpoint: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        if let Some(cb) = breakers.get_mut(endpoint) {
            cb.record_success();
        }
    }

    /// Check if quota warning applies (>=80% used)
    pub async fn is_warning(&self, user_id: &str) -> bool {
        let now = Utc::now();
        let limits = self.user_limits.read().await;

        if let Some(user) = limits.get(user_id) {
            let day_used = user.daily_window.count(now);
            is_quota_warning(day_used, user.daily_window.limit)
        } else {
            false
        }
    }
}

// ── Usage Stats (re-export from models for handlers) ─────────────────────

pub use crate::api::models::UsageStats;
