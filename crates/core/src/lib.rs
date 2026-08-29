//! Ranking math and card rendering.
//!
//! This crate is the single source of truth for how a rank is computed and how
//! a card is drawn. It is compiled twice: natively into the server, and to
//! WebAssembly for the frontend, so a card previewed in the browser is byte-for-
//! byte the card the badge endpoint serves.
//!
//! That dual target sets the rules here: **no I/O, no clock, no randomness, no
//! async.** Anything ambient — the current year, the request — is passed in by
//! the caller. Keeping this crate pure is what makes the two targets agree.

pub mod ranking;
pub mod render;
pub mod validation;

pub use ranking::{calculate_rank, AggregatedStats, Division, RankResult, Tier};
pub use validation::{parse_season, parse_theme, validate_username, Theme, ValidationError};
