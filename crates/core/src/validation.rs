//! Request parameter validation.
//!
//! Rules match the upstream Zod schemas so that requests which worked against
//! the Next.js service keep working, and requests it rejected keep failing.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Card colour scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Default,
    Dark,
    Light,
    Minimal,
    Cyberpunk,
    Ocean,
    Forest,
    Sunset,
    Galaxy,
}

impl Theme {
    pub const ALL: [Theme; 9] = [
        Theme::Default,
        Theme::Dark,
        Theme::Light,
        Theme::Minimal,
        Theme::Cyberpunk,
        Theme::Ocean,
        Theme::Forest,
        Theme::Sunset,
        Theme::Galaxy,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Default => "default",
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Minimal => "minimal",
            Theme::Cyberpunk => "cyberpunk",
            Theme::Ocean => "ocean",
            Theme::Forest => "forest",
            Theme::Sunset => "sunset",
            Theme::Galaxy => "galaxy",
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Theme {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Theme::ALL
            .into_iter()
            .find(|t| t.as_str().eq_ignore_ascii_case(s))
            .ok_or(())
    }
}

/// Parse a theme, falling back to the default for anything unrecognised.
///
/// Deliberately lenient: a bad `?theme=` in someone's README should still render
/// a card rather than replace their badge with a broken image.
pub fn parse_theme(raw: Option<&str>) -> Theme {
    raw.and_then(|s| s.parse().ok()).unwrap_or_default()
}

/// A rejected request parameter.
///
/// Core stays free of any HTTP or transport concern; the server maps these onto
/// status codes and the wire format in `error.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    Username { value: String },
    Season { value: String, current_year: i32 },
}

impl ValidationError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Username { .. } => "Invalid GitHub username format",
            Self::Season { .. } => "Invalid season parameter",
        }
    }

    /// The offending value, echoed back so the caller can see what we read.
    pub fn value(&self) -> &str {
        match self {
            Self::Username { value } | Self::Season { value, .. } => value,
        }
    }

    /// Human-readable statement of the rule that was broken.
    pub fn hint(&self) -> String {
        match self {
            Self::Username { .. } => "Must be 1-39 alphanumeric characters or hyphens".to_string(),
            Self::Season { current_year, .. } => {
                format!("Season must be a year between {MIN_SEASON} and {current_year}")
            }
        }
    }

    /// The query parameter at fault.
    pub fn field(&self) -> &'static str {
        match self {
            Self::Username { .. } => "username",
            Self::Season { .. } => "season",
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for ValidationError {}

pub type ValidationResult<T> = Result<T, ValidationError>;

/// GitHub's username rule: 1–39 alphanumerics or hyphens, no leading, trailing
/// or doubled hyphen.
pub fn is_valid_username(username: &str) -> bool {
    let bytes = username.as_bytes();

    if bytes.is_empty() || bytes.len() > 39 {
        return false;
    }

    let alnum = |b: u8| b.is_ascii_alphanumeric();

    if !alnum(bytes[0]) || !alnum(bytes[bytes.len() - 1]) {
        return false;
    }

    // GitHub itself also rejects consecutive hyphens.
    bytes.windows(2).all(|w| w != b"--") && bytes.iter().all(|&b| alnum(b) || b == b'-')
}

pub fn validate_username(username: &str) -> ValidationResult<()> {
    if is_valid_username(username) {
        return Ok(());
    }

    Err(ValidationError::Username {
        value: username.to_string(),
    })
}

/// Earliest season we accept — GitHub's contribution graph does not go back
/// meaningfully further.
pub const MIN_SEASON: i32 = 2010;

/// Parse a `?season=` year, if one was supplied.
///
/// Next year is allowed so the service does not break at the new year boundary
/// for clients running ahead of UTC.
pub fn parse_season(raw: Option<&str>, current_year: i32) -> ValidationResult<Option<i32>> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    let season_error = || ValidationError::Season {
        value: raw.to_string(),
        current_year,
    };

    let year: i32 = raw.trim().parse().map_err(|_| season_error())?;

    // Next year is allowed so clients running ahead of UTC don't break at the
    // new year boundary.
    if !(MIN_SEASON..=current_year + 1).contains(&year) {
        return Err(season_error());
    }

    Ok(Some(year))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_github_usernames() {
        for name in [
            "a",
            "octocat",
            "Shemarhn",
            "torvalds",
            "a-b-c",
            "user123",
            &"a".repeat(39),
        ] {
            assert!(is_valid_username(name), "{name} should be valid");
        }
    }

    #[test]
    fn rejects_malformed_usernames() {
        for name in [
            "",
            "-lead",
            "trail-",
            "a--b",
            "has space",
            "has_underscore",
            "dot.dot",
            &"a".repeat(40),
        ] {
            assert!(!is_valid_username(name), "{name:?} should be rejected");
        }
    }

    #[test]
    fn unknown_theme_falls_back_rather_than_failing() {
        assert_eq!(parse_theme(Some("cyberpunk")), Theme::Cyberpunk);
        assert_eq!(parse_theme(Some("CYBERPUNK")), Theme::Cyberpunk);
        assert_eq!(parse_theme(Some("nonsense")), Theme::Default);
        assert_eq!(parse_theme(None), Theme::Default);
    }

    #[test]
    fn season_bounds_match_upstream() {
        assert_eq!(parse_season(None, 2026).unwrap(), None);
        assert_eq!(parse_season(Some("2024"), 2026).unwrap(), Some(2024));
        assert_eq!(parse_season(Some("2010"), 2026).unwrap(), Some(2010));
        assert_eq!(parse_season(Some("2027"), 2026).unwrap(), Some(2027));

        assert!(parse_season(Some("2009"), 2026).is_err());
        assert!(parse_season(Some("2028"), 2026).is_err());
        assert!(parse_season(Some("notayear"), 2026).is_err());
    }
}
