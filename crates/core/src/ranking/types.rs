use serde::{Deserialize, Serialize};
use std::fmt;

/// Competitive tier, ordered lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    Iron,
    Bronze,
    Silver,
    Gold,
    Platinum,
    Emerald,
    Diamond,
    Master,
    Grandmaster,
    Challenger,
}

impl Tier {
    /// Every tier, highest first — the order `get_tier` scans in.
    pub const ALL_DESC: [Tier; 10] = [
        Tier::Challenger,
        Tier::Grandmaster,
        Tier::Master,
        Tier::Diamond,
        Tier::Emerald,
        Tier::Platinum,
        Tier::Gold,
        Tier::Silver,
        Tier::Bronze,
        Tier::Iron,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Iron => "Iron",
            Tier::Bronze => "Bronze",
            Tier::Silver => "Silver",
            Tier::Gold => "Gold",
            Tier::Platinum => "Platinum",
            Tier::Emerald => "Emerald",
            Tier::Diamond => "Diamond",
            Tier::Master => "Master",
            Tier::Grandmaster => "Grandmaster",
            Tier::Challenger => "Challenger",
        }
    }

    /// Master and above are undivided.
    pub fn has_divisions(self) -> bool {
        self < Tier::Master
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Division within a tier. `I` is the highest, `IV` the lowest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Division {
    IV,
    III,
    II,
    I,
}

impl Division {
    /// Index from the bottom of the tier: IV = 0 … I = 3.
    pub fn index(self) -> u32 {
        match self {
            Division::IV => 0,
            Division::III => 1,
            Division::II => 2,
            Division::I => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Division::IV => "IV",
            Division::III => "III",
            Division::II => "II",
            Division::I => "I",
        }
    }
}

impl fmt::Display for Division {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Contribution totals a rank is computed from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedStats {
    #[serde(rename = "totalMergedPRs")]
    pub total_merged_prs: f64,
    pub total_code_reviews: f64,
    pub total_issues_closed: f64,
    pub total_commits: f64,
    pub total_stars: f64,
    #[serde(default)]
    pub total_followers: f64,
    #[serde(default)]
    pub first_contribution_year: i32,
    #[serde(default)]
    pub last_contribution_year: i32,
    #[serde(default)]
    pub years_active: i32,
}

/// The full result of ranking a user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankResult {
    pub tier: Tier,
    pub division: Option<Division>,
    pub elo: i64,
    pub gp: i64,
    pub percentile: f64,
    pub wpi: f64,
    pub z_score: f64,
}
