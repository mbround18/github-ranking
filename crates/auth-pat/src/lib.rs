//! Personal access token pool.
//!
//! Round-robins a set of PATs, skipping any GitHub has told us are out of quota
//! until their window resets.
//!
//! This is the development path, and the right choice for a single-tenant
//! self-hosted instance where the quota being spent is the operator's own. It is
//! *not* appropriate for a public deployment: every visitor would share one
//! token's 5,000 points per hour.

use github_ranked_auth_core::{
    now_unix, AuthError, AuthProvider, AuthResult, Credential, CredentialId, CredentialKind,
    RateLimitStatus, GRAPHQL_POINTS_PER_HOUR,
};
use std::sync::Mutex;

/// Whether this build permits personal access tokens in production.
///
/// A compile-time constant rather than configuration: it cannot be flipped by an
/// environment variable, so a binary built without the feature is incapable of
/// running production traffic on a PAT even if one is supplied.
#[cfg(feature = "allow-in-production")]
pub const ALLOWED_IN_PRODUCTION: bool = true;
#[cfg(not(feature = "allow-in-production"))]
pub const ALLOWED_IN_PRODUCTION: bool = false;

struct TokenState {
    token: String,
    /// Best known remaining quota. Estimated between responses, corrected by
    /// [`AuthProvider::record`] whenever GitHub reports the real figure.
    remaining: u32,
    /// Unix seconds at which `remaining` resets. Zero until GitHub says.
    reset_at: i64,
}

/// Redacted, so a pool can be `Debug`-printed without spilling tokens.
impl std::fmt::Debug for TokenState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenState")
            .field("token", &"<redacted>")
            .field("remaining", &self.remaining)
            .field("reset_at", &self.reset_at)
            .finish()
    }
}

#[derive(Debug)]
pub struct PatProvider {
    state: Mutex<Vec<TokenState>>,
    next: Mutex<usize>,
}

impl PatProvider {
    /// Build a pool from explicit tokens.
    pub fn new(tokens: Vec<String>) -> AuthResult<Self> {
        if tokens.is_empty() {
            return Err(AuthError::Misconfigured(
                "no GitHub tokens configured; set GITHUB_TOKEN or GITHUB_TOKEN_1".into(),
            ));
        }

        let state = tokens
            .into_iter()
            .map(|token| TokenState {
                token,
                remaining: GRAPHQL_POINTS_PER_HOUR,
                reset_at: 0,
            })
            .collect();

        Ok(Self {
            state: Mutex::new(state),
            next: Mutex::new(0),
        })
    }

    /// Read `GITHUB_TOKEN`, then `GITHUB_TOKEN_1`, `GITHUB_TOKEN_2`, … until a gap.
    ///
    /// In production this requires the `allow-in-production` feature, so a
    /// deployment cannot silently fall back off the GitHub App.
    pub fn from_env(is_production: bool) -> AuthResult<Self> {
        if is_production && !ALLOWED_IN_PRODUCTION {
            return Err(AuthError::Misconfigured(
                "personal access tokens are not permitted in production; rebuild with \
                 --features pat-in-production for a single-tenant instance, or configure \
                 the GitHub App"
                    .into(),
            ));
        }

        if is_production {
            // Loud, because the quota is one shared token rather than per-user,
            // and it will not scale past a single-tenant instance.
            tracing::warn!(
                "authenticating with a personal access token in production (build feature \
                 `pat-in-production`); every visitor shares this token's rate limit"
            );
        }

        let mut tokens: Vec<String> = std::env::var("GITHUB_TOKEN").ok().into_iter().collect();

        for index in 1.. {
            match std::env::var(format!("GITHUB_TOKEN_{index}")) {
                Ok(token) if !token.trim().is_empty() => tokens.push(token),
                _ => break,
            }
        }

        tracing::info!(count = tokens.len(), "loaded personal access tokens");
        Self::new(tokens)
    }

    /// Whether a slot has quota, resetting it if its window has elapsed.
    fn has_quota(entry: &mut TokenState, now: i64) -> bool {
        if entry.remaining > 0 {
            return true;
        }

        if now >= entry.reset_at {
            entry.remaining = GRAPHQL_POINTS_PER_HOUR;
            return true;
        }

        false
    }

    fn position(&self, id: CredentialId) -> Option<usize> {
        (id.kind == CredentialKind::Pat).then_some(id.index)
    }

    /// Number of tokens in the pool, regardless of quota.
    pub fn len(&self) -> usize {
        self.state.lock().expect("token pool poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait::async_trait]
impl AuthProvider for PatProvider {
    async fn credential(&self) -> AuthResult<Credential> {
        let now = now_unix();
        let mut state = self.state.lock().expect("token pool poisoned");
        let mut next = self.next.lock().expect("token cursor poisoned");

        let count = state.len();
        let start = *next;

        for offset in 0..count {
            let index = (start + offset) % count;
            if Self::has_quota(&mut state[index], now) {
                *next = (index + 1) % count;
                return Ok(Credential::new(
                    CredentialId { kind: CredentialKind::Pat, index },
                    state[index].token.clone(),
                ));
            }
        }

        // Everything is spent — tell the caller when the earliest window reopens.
        let retry_after = state
            .iter()
            .map(|entry| entry.reset_at)
            .min()
            .map(|reset| (reset - now).max(0) as u64);

        Err(AuthError::Exhausted { retry_after })
    }

    fn record(&self, id: CredentialId, status: RateLimitStatus) {
        let Some(index) = self.position(id) else { return };

        let mut state = self.state.lock().expect("token pool poisoned");
        if let Some(entry) = state.get_mut(index) {
            entry.remaining = status.remaining;
            entry.reset_at = status.reset_at;
        }
    }

    fn record_spend(&self, id: CredentialId, points: u32) {
        let Some(index) = self.position(id) else { return };

        let mut state = self.state.lock().expect("token pool poisoned");
        if let Some(entry) = state.get_mut(index) {
            entry.remaining = entry.remaining.saturating_sub(points);
        }
    }

    fn available(&self) -> usize {
        let now = now_unix();
        let mut state = self.state.lock().expect("token pool poisoned");
        state
            .iter_mut()
            .filter(|entry| entry.remaining > 0 || (entry.reset_at != 0 && now >= entry.reset_at))
            .count()
    }

    fn kind(&self) -> CredentialKind {
        CredentialKind::Pat
    }
}
