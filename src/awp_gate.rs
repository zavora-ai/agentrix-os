use std::collections::HashMap;
use std::sync::Arc;

use adk_awp::error_response::awp_error_response;
use adk_awp::{
    DefaultTrustAssigner, InMemoryRateLimiter, RateLimitConfig, RateLimiter, TrustLevelAssigner,
};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use awp_types::{AwpError, BusinessContext, TrustLevel};
use axum::http::HeaderMap;
use axum::response::Response;

use crate::auth;

/// Shared AWP security: trust assignment, rate limits, capability tiers.
pub struct AwpGate {
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub trust_assigner: Arc<dyn TrustLevelAssigner>,
    pub business_context: Arc<ArcSwap<BusinessContext>>,
    jwt_secret: Option<String>,
}

impl AwpGate {
    pub fn new(
        jwt_secret: Option<String>,
        business_context: Arc<ArcSwap<BusinessContext>>,
    ) -> Self {
        let mut limits = HashMap::new();
        limits.insert(
            TrustLevel::Anonymous,
            RateLimitConfig {
                max_requests: 60,
                window_secs: 60,
            },
        );
        limits.insert(
            TrustLevel::Known,
            RateLimitConfig {
                max_requests: 120,
                window_secs: 60,
            },
        );
        limits.insert(
            TrustLevel::Partner,
            RateLimitConfig {
                max_requests: 600,
                window_secs: 60,
            },
        );

        let trust_assigner: Arc<dyn TrustLevelAssigner> = Arc::new(JwtTrustAssigner {
            jwt_secret: jwt_secret.clone(),
            fallback: DefaultTrustAssigner,
        });

        Self {
            rate_limiter: Arc::new(InMemoryRateLimiter::with_config(limits)),
            trust_assigner,
            business_context,
            jwt_secret,
        }
    }

    /// The JWT user behind Known trust (`auth::extract_user_id` with this gate's secret).
    pub fn user_id(&self, headers: &HeaderMap) -> Option<uuid::Uuid> {
        auth::extract_user_id(headers, self.jwt_secret.as_deref()?)
    }

    pub async fn check(
        &self,
        headers: &HeaderMap,
        client_key: &str,
        capability: &str,
    ) -> Result<TrustLevel, Response> {
        let trust = self.trust_assigner.assign(headers).await;

        if let Err(retry_after_secs) = self.rate_limiter.check(client_key, trust).await {
            return Err(awp_error_response(AwpError::RateLimited { retry_after_secs }));
        }

        let ctx = self.business_context.load();
        let required = ctx
            .capabilities
            .iter()
            .find(|c| c.name == capability)
            .map(|c| c.access_level)
            .unwrap_or(TrustLevel::Anonymous);

        if trust < required {
            return Err(awp_error_response(AwpError::Forbidden(format!(
                "capability '{capability}' requires {required}, caller is {trust}"
            ))));
        }

        Ok(trust)
    }
}

pub struct JwtTrustAssigner {
    jwt_secret: Option<String>,
    fallback: DefaultTrustAssigner,
}

#[async_trait]
impl TrustLevelAssigner for JwtTrustAssigner {
    async fn assign(&self, headers: &HeaderMap) -> TrustLevel {
        if let Some(secret) = &self.jwt_secret
            && auth::extract_user_id(headers, secret).is_some()
        {
            return TrustLevel::Known;
        }
        self.fallback.assign(headers).await
    }
}

pub fn client_key(headers: &HeaderMap, fallback: &str) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| fallback.to_string())
}