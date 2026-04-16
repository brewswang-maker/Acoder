//! Webhook HTTP Handler — 接收并处理外部 Webhook 事件

use crate::webhook::{WebhookEvent, WebhookRouter, WebhookConfig};
use crate::Result;
use axum::{
    extract::Request,
    routing::post,
    Json, Router,
};
use bytes::Bytes;
use hyper::header::HeaderValue;
use std::sync::Arc;
use tokio::sync::RwLock;

const GITHUB_EVENT_HEADER: &str = "X-GitHub-Event";
const GITHUB_SIGNATURE_HEADER: &str = "X-Hub-Signature-256";
const GITLAB_EVENT_HEADER: &str = "X-Gitlab-Event";

/// Webhook 应用状态
pub struct WebhookState {
    pub router: Arc<RwLock<WebhookRouter>>,
    pub config: WebhookConfig,
}

impl WebhookState {
    pub fn new(router: WebhookRouter, config: WebhookConfig) -> Self {
        Self {
            router: Arc::new(RwLock::new(router)),
            config,
        }
    }
}

/// GitHub Webhook 处理
pub async fn github_webhook(
    State(state): State<Arc<WebhookState>>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> &'static str {
    // 获取事件类型
    let event_type = headers
        .get(GITHUB_EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("push");

    // 验证签名（如果配置了 secret）
    if let Some(ref secret) = state.config.github_secret {
        let signature = headers
            .get(GITHUB_SIGNATURE_HEADER)
            .and_then(|v| v.to_str().ok());
        if !verify_github_signature(body.clone(), secret, signature) {
            return "Forbidden: Invalid signature";
        }
    }

    // 解析事件
    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to parse GitHub webhook payload: {}", e);
            return "Bad Request: Invalid JSON";
        }
    };

    // 路由事件
    let event = parse_github_event(event_type, &payload);
    if let Some(event) = event {
        let router = state.router.read().await;
        let matched_rules = router.match_event(&event);
        for rule in matched_rules {
            let prompt = router.build_prompt(rule, &event);
            tracing::info!(
                "GitHub webhook matched rule '{}' ({}), prompt: {}",
                rule.name,
                rule.id,
                prompt
            );
            // TODO: 触发 Agent 或任务（异步，不阻塞响应）
        }
    }

    "OK"
}

/// GitLab Webhook 处理
pub async fn gitlab_webhook(
    State(state): State<Arc<WebhookState>>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> &'static str {
    let event_type = headers
        .get(GITLAB_EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("Push Hook");

    // 验证签名（如果配置了 secret）
    if let Some(ref secret) = state.config.gitlab_secret {
        let token = headers
            .get("X-Gitlab-Token")
            .and_then(|v| v.to_str().ok());
        if token != Some(secret) {
            return "Forbidden: Invalid token";
        }
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to parse GitLab webhook payload: {}", e);
            return "Bad Request: Invalid JSON";
        }
    };

    let event = parse_gitlab_event(event_type, &payload);
    if let Some(event) = event {
        let router = state.router.read().await;
        let matched_rules = router.match_event(&event);
        for rule in matched_rules {
            let prompt = router.build_prompt(rule, &event);
            tracing::info!(
                "GitLab webhook matched rule '{}' ({}), prompt: {}",
                rule.name,
                rule.id,
                prompt
            );
        }
    }

    "OK"
}

/// 创建 Webhook 路由
pub fn create_webhook_routes(state: Arc<WebhookState>) -> Router {
    Router::new()
        .route("/webhooks/github", post(github_webhook))
        .route("/webhooks/gitlab", post(gitlab_webhook))
        .with_state(state)
}

// ─── 签名验证 ────────────────────────────────────────────────

fn verify_github_signature(body: Bytes, secret: &str, signature: Option<&str>) -> bool {
    use sha2::{Sha256, Digest};

    let Some(sig) = signature else {
        return false;
    };

    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(&body);
    let result = hasher.finalize();

    let expected = format!("sha256={}", hex::encode(result));
    expected == sig
}

// ─── 事件解析 ────────────────────────────────────────────────

fn parse_github_event(event_type: &str, payload: &serde_json::Value) -> Option<WebhookEvent> {
    let action = payload
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let repository = serde_json::from_value(payload.get("repository")?.clone()).ok()?;

    let sender = serde_json::from_value(payload.get("sender")?.clone()).ok()?;

    let pull_request = payload
        .get("pull_request")
        .map(|v| serde_json::from_value(v.clone()).ok())
        .flatten();

    let push = payload
        .get("push")
        .map(|v| serde_json::from_value(v.clone()).ok())
        .flatten();

    Some(WebhookEvent::GitHub(crate::webhook::GitHubEvent {
        action,
        repository,
        pull_request,
        push,
        sender,
    }))
}

fn parse_gitlab_event(event_type: &str, payload: &serde_json::Value) -> Option<WebhookEvent> {
    use super::GitLabEvent;

    let object_kind = payload
        .get("object_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let project = serde_json::from_value(payload.get("project")?.clone()).ok()?;

    let merge_request = payload
        .get("object_attributes")
        .filter(|_| object_kind == "merge_request")
        .map(|v| serde_json::from_value(v.clone()).ok())
        .flatten()
        .map(|attrs| crate::webhook::GitLabMR {
            iid: attrs.get("iid").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            title: attrs.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            source_branch: attrs
                .get("source_branch")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            target_branch: attrs
                .get("target_branch")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        });

    let push = if object_kind == "push" {
        let commits: Vec<crate::webhook::GitLabCommit> = payload
            .get("commits")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        Some(crate::webhook::GitLabCommit {
                            id: c.get("id")?.as_str()?.to_string(),
                            message: c.get("message")?.as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(crate::webhook::GitLabPush {
            ref_: payload
                .get("ref")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            commits,
        })
    } else {
        None
    };

    Some(WebhookEvent::GitLab(GitLabEvent {
        object_kind,
        project,
        merge_request,
        push,
    }))
}

use hex;
use serde::Deserialize;
