//! The credential contract every authentication provider implements.
//!
//! This crate deliberately contains **no providers** — only the trait and the
//! types it speaks in. Providers live in their own crates so a build can
//! include exactly the ones it is meant to have, and so each can be tested
//! without standing up an HTTP server.
//!
//! The service will eventually authenticate three different ways, and they are
//! not interchangeable:
//!
//! | Context | Credential | Quota |
//! | --- | --- | --- |
//! | Local development | personal access token | 5,000/hr, shared |
//! | Anonymous badge render | GitHub App installation token | 5,000/hr, shared |
//! | Signed-in user | user-to-server token | 5,000/hr, *per user* |
//!
//! Badge requests arrive through GitHub's camo proxy with no cookies, so they
//! can only ever use the first two. A signed-in user's token is for refreshing
//! *their own* cached entry with their own quota, never for arbitrary lookups.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// GitHub's GraphQL budget for one credential, per hour.
pub const GRAPHQL_POINTS_PER_HOUR: u32 = 5_000;

/// Seconds since the Unix epoch.
///
/// Providers track rate-limit windows in absolute time, so they need a clock.
/// It lives here rather than being passed in because every implementation would
/// otherwise thread the same parameter through every method.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Identifies a credential for rate-limit accounting, without revealing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CredentialId {
    pub kind: CredentialKind,
    pub index: usize,
}

impl fmt::Display for CredentialId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.kind.as_str(), self.index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CredentialKind {
    /// A personal access token.
    Pat,
    /// A GitHub App installation token — the app acting as itself.
    Installation,
    /// A user-to-server token — the app acting as a signed-in user.
    User,
}

impl CredentialKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pat => "pat",
            Self::Installation => "installation",
            Self::User => "user",
        }
    }
}

/// A bearer token plus the handle used to report its quota back.
///
/// `Debug` is implemented by hand: this must never reach a log line.
#[derive(Clone)]
pub struct Credential {
    pub id: CredentialId,
    token: String,
}

impl Credential {
    pub fn new(id: CredentialId, token: impl Into<String>) -> Self {
        Self { id, token: token.into() }
    }

    /// The secret itself. Only an HTTP client should call this.
    pub fn expose(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("id", &self.id)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// What GitHub told us about a credential's remaining quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitStatus {
    /// GraphQL points left in the current window.
    pub remaining: u32,
    /// Unix seconds at which the window resets.
    pub reset_at: i64,
}

/// Why a credential could not be issued.
///
/// Transport-agnostic: the service maps these onto status codes, so this crate
/// stays testable without an HTTP stack.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthError {
    #[error("{0}")]
    Misconfigured(String),

    #[error("all credentials are rate-limited")]
    Exhausted {
        /// Seconds until the earliest window reopens, when known.
        retry_after: Option<u64>,
    },

    #[error("credential rejected: {0}")]
    Rejected(String),
}

pub type AuthResult<T> = Result<T, AuthError>;

/// Supplies credentials for outbound GitHub calls.
#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
    /// Lease a credential with quota available.
    async fn credential(&self) -> AuthResult<Credential>;

    /// Report the quota GitHub returned, so exhausted credentials are skipped
    /// until their window resets.
    fn record(&self, id: CredentialId, status: RateLimitStatus);

    /// Note that a credential was spent, for providers that estimate quota
    /// between authoritative responses.
    fn record_spend(&self, id: CredentialId, points: u32);

    /// How many credentials currently have quota. Surfaced on `/readyz`.
    fn available(&self) -> usize;

    fn kind(&self) -> CredentialKind;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_never_prints_its_token() {
        let credential = Credential::new(
            CredentialId { kind: CredentialKind::Pat, index: 3 },
            "ghp_supersecretvalue",
        );

        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("supersecret"), "token leaked: {rendered}");
        assert!(rendered.contains("<redacted>"));
        // The id is safe and useful, so it should still be there. Derived
        // `Debug` prints the variant name, not `as_str`.
        assert!(rendered.contains("Pat"), "id missing from {rendered}");
        assert!(rendered.contains('3'));
    }

    #[test]
    fn the_token_is_still_reachable_deliberately() {
        let credential = Credential::new(
            CredentialId { kind: CredentialKind::User, index: 0 },
            "gho_value",
        );
        assert_eq!(credential.expose(), "gho_value");
    }

    #[test]
    fn credential_ids_render_for_logs() {
        let id = CredentialId { kind: CredentialKind::Installation, index: 7 };
        assert_eq!(id.to_string(), "installation#7");
    }

    #[test]
    fn the_clock_is_plausible() {
        // Sanity: seconds, not millis.
        assert!(now_unix() > 1_700_000_000);
        assert!(now_unix() < 4_000_000_000);
    }
}
