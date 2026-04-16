//! API Middleware — Auth + Rate Limiting
//!
//! - AuthMiddleware: 从 Authorization: Bearer <token> 提取 JWT，验证后放入 Extension
//! - rate_limit_middleware: 调用 RateLimiter.check()，超限返回 429

use std::sync::Arc;
use tower::Service;
use axum::{
    extract::{Request, Extension},
    middleware::Next,
    response::Response,
    http::StatusCode,
};

use crate::gateway::auth::AuthService;

// ── Auth Middleware ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AuthMiddleware;

impl AuthMiddleware {
    pub fn new() -> Self {
        Self
    }
}

impl<S> tower::Layer<S> for AuthMiddleware {
    type Service = AuthMiddlewareSvc<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddlewareSvc { inner }
    }
}

#[derive(Clone)]
pub struct AuthMiddlewareSvc<S> {
    inner: S,
}

impl<S, B> tower::Service<Request<B>> for AuthMiddlewareSvc<S>
where
    S: tower::Service<Request<B>, Response = Response> + Clone + Send + Sync + 'static,
    <S as tower::Service<Request<B>>>::Future: std::marker::Send,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            let auth_header = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|h| h.strip_prefix("Bearer "));

            let service = AuthService::from_env();

            match auth_header {
                Some(token) if service.verify_token(token).is_ok() => {
                    let claims = service.verify_token(token).unwrap();
                    let mut req = request;
                    req.extensions_mut().insert(AuthenticatedUser {
                        user_id: claims.sub,
                    });
                    inner.call(req).await
                }
                _ => {
                    Ok(Response::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(
                            r#"{"success":false,"error":{"code":"UNAUTHORIZED","message":"Invalid or missing token"}}"#
                        ))
                        .unwrap())
                }
            }
        })
    }
}

/// Authenticated user info stored in request extensions
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
}

// ── Rate Limit Middleware ─────────────────────────────────────────────────

/// Check rate limit before processing request.
/// Must be used with `AuthenticatedUser` and `Arc<RateLimiter>` in extensions.
pub async fn rate_limit_middleware(
    Extension(user): Extension<AuthenticatedUser>,
    Extension(limiter): Extension<Arc<crate::api::rate_limiter::RateLimiter>>,
    mut request: Request,
    next: Next,
) -> Response {
    let user_id = &user.user_id;

    // Check circuit breaker (e.g. /api/v1/tasks)
    let path = request.uri().path().to_string();
    if !limiter.check_circuit(&path).await {
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("Content-Type", "application/json")
            .header("Retry-After", "60")
            .body(axum::body::Body::from(
                r#"{"success":false,"error":{"code":"CIRCUIT_OPEN","message":"Service temporarily unavailable due to high error rate"}}"#
            ))
            .unwrap();
    }

    // Check rate limit
    let result = limiter.check(user_id).await;

    if !result.allowed {
        let response = Response::builder()
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
        return response;
    }

    // Record the request
    limiter.record(user_id).await;

    // Add rate limit headers to request for handlers
    let mut req = request;
    req.extensions_mut().insert(RateLimitHeaders {
        limit: result.limit,
        remaining: result.remaining,
        reset_in_secs: result.reset_in_secs,
    });

    let mut response = next.run(req).await;

    // Add rate limit headers to response
    let headers = response.headers();
    if !headers.contains_key("X-RateLimit-Limit") {
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
    }

    response
}

/// Rate limit info stored in request extensions for handlers
#[derive(Debug, Clone)]
pub struct RateLimitHeaders {
    pub limit: usize,
    pub remaining: usize,
    pub reset_in_secs: u64,
}
