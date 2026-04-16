//! API Routes — Router assembly with middleware stack
//!
//! Design: 产品设计文档 v3.0 §11.5.7
//! Middleware stack (bottom → top):
//!   1. Rate limiting (per-user sliding window)
//!   2. Auth (JWT Bearer token)

use std::sync::Arc;
use axum::{
    Router,
    middleware::Next,
    extract::{Request, Extension},
    response::Response,
    http::StatusCode,
};

use crate::api::handlers::{self, AppState};
use crate::api::rate_limiter::RateLimiter;
use crate::api::middleware::AuthenticatedUser;

// ── Middleware ────────────────────────────────────────────────────────────

async fn auth_layer(mut request: Request, next: Next) -> Response {
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let service = crate::gateway::auth::AuthService::from_env();

    match auth_header {
        Some(token) if service.verify_token(token).is_ok() => {
            let claims = service.verify_token(token).unwrap();
            request.extensions_mut().insert(AuthenticatedUser {
                user_id: claims.sub,
            });
            next.run(request).await
        }
        _ => {
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"success":false,"error":{"code":"UNAUTHORIZED","message":"Invalid or missing token"}}"#
                ))
                .unwrap()
        }
    }
}

/// Rate limit middleware: extracts AuthenticatedUser and RateLimiter from extensions
async fn rate_limit_layer(request: Request, next: Next) -> Response {
    let limiter = request
        .extensions()
        .get::<Arc<RateLimiter>>()
        .cloned();
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .cloned();

    let (Some(limiter), Some(user)) = (limiter, user) else {
        // No auth/rate-limit context — pass through (should not happen in normal flow)
        return next.run(request).await;
    };

    let user_id = &user.user_id;

    // Check circuit breaker
    let path = request.uri().path().to_string();
    if !limiter.check_circuit(&path).await {
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("Content-Type", "application/json")
            .header("Retry-After", "60")
            .body(axum::body::Body::from(
                r#"{"success":false,"error":{"code":"CIRCUIT_OPEN","message":"Service temporarily unavailable"}}"#
            ))
            .unwrap();
    }

    // Check rate limit
    let result = limiter.check(user_id).await;

    if !result.allowed {
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Content-Type", "application/json")
            .header("X-RateLimit-Limit", result.limit.to_string())
            .header("X-RateLimit-Remaining", "0")
            .header("X-RateLimit-Reset", result.reset_in_secs.to_string())
            .header("Retry-After", result.reset_in_secs.to_string())
            .body(axum::body::Body::from(format!(
                r#"{{"success":false,"error":{{"code":"RATE_LIMITED","message":"Rate limit exceeded","details":"Try again in {} seconds"}}}}"#,
                result.reset_in_secs
            )))
            .unwrap();
    }

    // Record and add headers
    limiter.record(user_id).await;

    let mut response = next.run(request).await;
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-ratelimit-limit"),
        result.limit.to_string().parse().unwrap(),
    );
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-ratelimit-remaining"),
        result.remaining.to_string().parse().unwrap(),
    );
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-ratelimit-reset"),
        result.reset_in_secs.to_string().parse().unwrap(),
    );
    response
}

/// Create the fully-wired API router with auth + rate limiting middleware
pub fn create_routes() -> Router {
    let limiter = Arc::new(RateLimiter::new(
        crate::api::rate_limiter::RateLimitConfig::default(),
    ));

    let state = handlers::AppState::new(limiter.clone());

    // The API router has its own state (task store + rate limiter)
    // Auth middleware runs first (populates AuthenticatedUser extension)
    // Rate limit middleware runs second (reads AuthenticatedUser + RateLimiter extensions)
    let api_router = handlers::create_api_router(state)
        .layer(axum::middleware::from_fn(auth_layer))
        .layer(axum::middleware::from_fn(rate_limit_layer));

    Router::new()
        .nest("/api/v1", api_router)
        // Health endpoint at root (no auth)
        .route("/health", axum::routing::get(handlers::health))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routes_compile() {
        let _router = create_routes();
    }
}
