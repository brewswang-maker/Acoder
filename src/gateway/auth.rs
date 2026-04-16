//! 认证中间件模块
//!
//! 基于 axum 的认证层，支持两种模式：
//! - API Key 认证（简单模式）：直接比对静态 key
//! - JWT 认证（生产模式）：HMAC-SHA256 签名验证
//!
//! Token 从环境变量 ACODE_API_KEY 读取，默认 "acode-default-key"。
//! 使用 hmac + sha2 实现 JWT，不引入额外依赖。

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::{Error, Result};

/// HMAC-SHA256 类型别名
type HmacSha256 = Hmac<Sha256>;

// ── Claims ────────────────────────────────────────────────────────────────

/// JWT Claims 结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 主体（用户 ID）
    pub sub: String,
    /// 过期时间（UNIX 时间戳，秒）
    pub exp: u64,
    /// 签发时间（UNIX 时间戳，秒）
    pub iat: u64,
}

impl Claims {
    /// 判断 token 是否已过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.exp < now
    }
}

// ── 认证模式 ──────────────────────────────────────────────────────────────

/// 认证模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// API Key 简单认证
    ApiKey,
    /// JWT 认证
    Jwt,
}

// ── 认证服务 ──────────────────────────────────────────────────────────────

/// 认证服务
#[derive(Clone)]
pub struct AuthService {
    /// 认证模式
    mode: AuthMode,
    /// API Key（简单模式）
    api_key: String,
    /// HMAC 密钥（JWT 模式）
    hmac_secret: Vec<u8>,
    /// Token 有效期（秒），默认 24 小时
    token_ttl_secs: u64,
}

impl AuthService {
    /// 从环境变量创建认证服务
    pub fn from_env() -> Self {
        let api_key = std::env::var("ACODE_API_KEY")
            .unwrap_or_else(|_| "acode-default-key".into());

        Self::with_config(&api_key, AuthMode::ApiKey, 86400)
    }

    /// 使用指定配置创建认证服务
    pub fn with_config(secret: &str, mode: AuthMode, token_ttl_secs: u64) -> Self {
        // 从 secret 派生 HMAC 密钥（取 SHA-256 哈希作为密钥）
        use sha2::Digest;
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        let hmac_secret = hasher.finalize().to_vec();

        Self {
            mode,
            api_key: secret.to_string(),
            hmac_secret,
            token_ttl_secs,
        }
    }

    /// 获取当前认证模式
    pub fn mode(&self) -> AuthMode {
        self.mode
    }

    /// 验证 token，返回 Claims
    pub fn verify_token(&self, token: &str) -> Result<Claims> {
        match self.mode {
            AuthMode::ApiKey => {
                if token == self.api_key {
                    Ok(Claims {
                        sub: "api-user".into(),
                        exp: u64::MAX,
                        iat: 0,
                    })
                } else {
                    Err(Error::AuthFailed { reason: "Invalid API Key".into() })
                }
            }
            AuthMode::Jwt => self.verify_jwt(token),
        }
    }

    /// 生成 token
    pub fn generate_token(&self, user_id: &str) -> Result<String> {
        match self.mode {
            AuthMode::ApiKey => Ok(self.api_key.clone()),
            AuthMode::Jwt => self.generate_jwt(user_id),
        }
    }

    /// 生成 JWT token
    ///
    /// header 和 payload 使用 base64url 无填充编码
    /// signature 使用 HMAC-SHA256
    fn generate_jwt(&self, user_id: &str) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let exp = if self.token_ttl_secs == 0 {
            // TTL=0：设为一个过去的值，确保立即过期
            now.saturating_sub(1)
        } else {
            now + self.token_ttl_secs
        };

        let claims = Claims {
            sub: user_id.to_string(),
            exp,
            iat: now,
        };

        let header = r#"{"alg":"HS256","typ":"JWT"}"#;
        let payload = serde_json::to_string(&claims)
            .map_err(|e| Error::AuthFailed {
                reason: format!("Claims 序列化失败: {}", e),
            })?;

        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());

        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let signature = self.sign(&signing_input);

        Ok(format!("{}.{}", signing_input, signature))
    }

    /// 验证 JWT token
    fn verify_jwt(&self, token: &str) -> Result<Claims> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(Error::AuthFailed {
                reason: "JWT 格式无效".into(),
            });
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);

        // 验证签名
        let expected_sig = self.sign(&signing_input);
        if expected_sig != parts[2] {
            return Err(Error::AuthFailed {
                reason: "JWT 签名无效".into(),
            });
        }

        // 解码 payload
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| Error::AuthFailed {
                reason: format!("JWT payload 解码失败: {}", e),
            })?;

        let claims: Claims = serde_json::from_slice(&payload_bytes).map_err(|e| {
            Error::AuthFailed {
                reason: format!("JWT claims 解析失败: {}", e),
            }
        })?;

        // 检查过期
        if claims.is_expired() {
            return Err(Error::AuthFailed {
                reason: "JWT 已过期".into(),
            });
        }

        Ok(claims)
    }

    /// HMAC-SHA256 签名
    fn sign(&self, data: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.hmac_secret)
            .expect("HMAC 密钥长度正确（SHA-256 支持任意长度）");
        mac.update(data.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }
}

// ── Axum 中间件 ───────────────────────────────────────────────────────────

/// 认证中间件处理器
pub async fn auth_middleware(
    request: Request,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let token = auth_header
        .and_then(|h| h.strip_prefix("Bearer "))
        .or_else(|| auth_header);

    let service = AuthService::from_env();

    match token {
        Some(t) if service.verify_token(t).is_ok() => next.run(request).await,
        _ => axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(axum::body::Body::from("Unauthorized"))
            .unwrap(),
    }
}

// ── 工具函数 ─────────────────────────────────────────────────────────────

/// 生成随机 API Key
pub fn generate_api_key() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    URL_SAFE_NO_PAD.encode(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_generate_and_verify() {
        let service = AuthService::with_config("secret", AuthMode::Jwt, 3600);
        let token = service.generate_token("user-1").unwrap();
        let claims = service.verify_token(&token).unwrap();
        assert_eq!(claims.sub, "user-1");
    }

    #[test]
    fn test_jwt_expired() {
        // TTL=0 → 生成的 token 立即过期
        let service = AuthService::with_config("secret", AuthMode::Jwt, 0);
        let token = service.generate_token("user-123").unwrap();
        // 验证：已过期的 token 应返回错误
        let result = service.verify_token(&token);
        assert!(result.is_err(), "TTL=0 的 token 应该已过期，验证应失败");
    }

    #[test]
    fn test_jwt_format() {
        let service = AuthService::with_config("secret", AuthMode::Jwt, 3600);
        let token = service.generate_token("user-1").unwrap();
        assert_eq!(token.split('.').count(), 3);
    }
}
