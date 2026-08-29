//! API errors.
//!
//! The JSON shape and status codes here are copied from the original service.
//! They are part of the public contract — anything already consuming the API
//! should not be able to tell the backend was replaced.

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    Validation {
        message: String,
        details: Map<String, Value>,
    },

    #[error("User not found: {username}")]
    UserNotFound { username: String },

    #[error("{message}")]
    RateLimit {
        message: String,
        retry_after: Option<u64>,
    },

    #[error("{message}")]
    GitHubApi { message: String },

    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            details: Map::new(),
        }
    }

    /// Attach a structured hint to a validation error, e.g. the expected format.
    pub fn with_detail(mut self, key: &str, value: impl Into<Value>) -> Self {
        if let Self::Validation { details, .. } = &mut self {
            details.insert(key.to_string(), value.into());
        }
        self
    }

    pub fn rate_limited(message: impl Into<String>, retry_after: Option<u64>) -> Self {
        Self::RateLimit {
            message: message.into(),
            retry_after,
        }
    }

    pub fn github(message: impl Into<String>) -> Self {
        Self::GitHubApi {
            message: message.into(),
        }
    }

    /// The upstream error name, which clients may switch on.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "ValidationError",
            Self::UserNotFound { .. } => "UserNotFound",
            Self::RateLimit { .. } => "RateLimitExceeded",
            Self::GitHubApi { .. } => "GitHubAPIError",
            Self::Internal(_) => "InternalError",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation { .. } => StatusCode::BAD_REQUEST,
            Self::UserNotFound { .. } => StatusCode::NOT_FOUND,
            Self::RateLimit { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::GitHubApi { .. } => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// How long a client should wait before retrying, when we know.
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            Self::RateLimit { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// Seconds this error may be cached for. Missing users are cached for an
    /// hour so a typo'd badge in a popular README can't hammer the GitHub API.
    pub fn cache_ttl(&self) -> u64 {
        match self {
            Self::UserNotFound { .. } => crate::cache::TTL_NOT_FOUND,
            _ => crate::cache::TTL_ERROR,
        }
    }

    fn body(&self, request_id: &str) -> ErrorBody {
        let details = match self {
            Self::Validation { details, .. } if !details.is_empty() => {
                Some(Value::Object(details.clone()))
            }
            Self::UserNotFound { username } => Some(json!({ "username": username })),
            _ => None,
        };

        ErrorBody {
            error: self.name(),
            code: self.status().as_u16(),
            // Internal failures are logged in full but never echoed to the
            // client — they can carry token or upstream detail.
            message: match self {
                Self::Internal(_) => "Internal server error".to_string(),
                other => other.to_string(),
            },
            details,
            request_id: request_id.to_string(),
            retry_after: self.retry_after(),
        }
    }

    /// Render as an HTTP response, tagged with the request id for correlation.
    pub fn into_response_with_id(self, request_id: &str) -> Response {
        if let Self::Internal(detail) = &self {
            tracing::error!(request_id, detail, "internal error");
        }

        let status = self.status();
        let retry_after = self.retry_after();
        let cache = format!("public, max-age={}", self.cache_ttl());
        let body = axum::Json(self.body(request_id));

        let mut response = (status, body).into_response();
        let headers = response.headers_mut();
        headers.insert(header::CACHE_CONTROL, cache.parse().expect("ascii"));
        headers.insert("x-request-id", request_id.parse().expect("uuid is ascii"));
        if let Some(seconds) = retry_after {
            headers.insert(header::RETRY_AFTER, seconds.into());
        }
        response
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let request_id = uuid::Uuid::new_v4().to_string();
        self.into_response_with_id(&request_id)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    error: &'static str,
    code: u16,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
    request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after: Option<u64>,
}

/// Lift a core validation failure into an HTTP error, preserving the field and
/// hint the core layer produced.
impl From<github_ranked_core::ValidationError> for ApiError {
    fn from(error: github_ranked_core::ValidationError) -> Self {
        ApiError::validation(error.message())
            .with_detail(error.field(), error.value())
            .with_detail("hint", error.hint())
    }
}

/// Auth failures are transport-agnostic by design; this is where they acquire
/// a status code.
impl From<github_ranked_auth_core::AuthError> for ApiError {
    fn from(error: github_ranked_auth_core::AuthError) -> Self {
        use github_ranked_auth_core::AuthError;

        match error {
            // A misconfigured credential is our problem, not the caller's.
            AuthError::Misconfigured(message) => ApiError::Internal(message),
            AuthError::Exhausted { retry_after } => ApiError::rate_limited(
                "All GitHub credentials are rate-limited. Please try again later.",
                retry_after,
            ),
            AuthError::Rejected(message) => ApiError::github(message),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
