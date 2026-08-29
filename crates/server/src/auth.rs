//! Authentication providers.
//!
//! The contract and the providers live in their own crates
//! (`crates/auth-core`, `crates/auth-pat`) so each can be tested without an HTTP
//! server, and so a build links only the providers it is meant to have. This
//! module is the seam that maps them onto the service's error type.

pub use github_ranked_auth_core::{
    AuthError, AuthProvider, Credential, CredentialId, CredentialKind, RateLimitStatus,
};

#[cfg(feature = "pat-auth")]
pub use github_ranked_auth_pat::PatProvider;

/// Whether this build can authenticate with a personal access token in
/// production.
///
/// False when the provider is not linked at all, which is the point: the
/// capability is structural rather than configured.
pub const fn pat_allowed_in_production() -> bool {
    #[cfg(feature = "pat-auth")]
    {
        github_ranked_auth_pat::ALLOWED_IN_PRODUCTION
    }
    #[cfg(not(feature = "pat-auth"))]
    {
        false
    }
}

/// Whether any provider is compiled in. A binary with none cannot serve a cache
/// miss, so this is checked at startup rather than failing on first request.
pub const fn any_provider_available() -> bool {
    cfg!(feature = "pat-auth")
}
