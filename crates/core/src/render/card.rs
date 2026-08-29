//! Card layout and SVG assembly.
//!
//! The card is a fixed 495x170, so positions are computed directly rather than
//! by running a flexbox solver. That is the whole reason Satori could be
//! dropped: it was buying a layout engine for a layout that never changes size.

use super::icons::tier_icon;
use super::text::{measure, text_svg, Anchor, Weight};
use super::theme::{
    ensure_contrast, palette, readable_tier_color, Palette, CONTRAST_BODY, STAT_COLORS,
};
use crate::ranking::constants::tier_colors;
use crate::ranking::RankResult;
use crate::validation::Theme;
use crate::AggregatedStats;

pub const CARD_WIDTH: f64 = 495.0;
pub const CARD_HEIGHT: f64 = 170.0;
const PADDING: f64 = 20.0;
const CORNER_RADIUS: f64 = 16.0;

const HEADER_HEIGHT: f64 = 16.0;
const HEADER_GAP: f64 = 12.0;

const ICON_SIZE: f64 = 80.0;
const COLUMN_GAP: f64 = 20.0;
const PANEL_WIDTH: f64 = 130.0;
const PANEL_PADDING: f64 = 12.0;

const STAT_ROW_HEIGHT: f64 = 14.0;
const STAT_ROW_GAP: f64 = 8.0;

const PROGRESS_HEIGHT: f64 = 6.0;

/// Everything needed to draw a card.
pub struct CardInput<'a> {
    pub username: &'a str,
    pub rank: &'a RankResult,
    pub stats: &'a AggregatedStats,
    pub theme: Theme,
    /// The season being shown, or `None` for all-time.
    pub season: Option<i32>,
    /// Used for the season label when showing all-time.
    pub current_year: i32,
}

/// A short id unique to this card's *appearance*.
///
/// SVG ids are document-global and the first definition wins, so deriving this
/// from the tier alone was a bug: two Gold cards in different themes on one page
/// would both render with whichever background was defined first. The frontend
/// gallery does exactly that. Hashing the visual inputs keeps identical cards
/// sharing ids (harmless, and it dedupes) while distinct ones stay separate.
fn card_namespace(input: &CardInput<'_>) -> String {
    // FNV-1a, inlined to keep this crate dependency-free.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    };

    mix(input.username.as_bytes());
    mix(input.rank.tier.as_str().as_bytes());
    mix(input.theme.as_str().as_bytes());
    mix(&input.rank.elo.to_le_bytes());
    mix(&input.season.unwrap_or(input.current_year).to_le_bytes());

    format!("g{:x}-", hash & 0xffff_ffff)
}

/// Baseline for text visually centred on `center_y`.
///
/// Cap height is roughly 0.7em, so half of it below the centre puts the optical
/// middle of the glyphs on the line.
fn baseline(center_y: f64, size: f64) -> f64 {
    center_y + size * 0.35
}

/// Shrink `size` until `text` fits `available`, so a long tier name can't spill
/// into the stats panel.
fn fit_size(text: &str, size: f64, weight: Weight, spacing: f64, available: f64) -> f64 {
    let width = measure(text, size, weight, spacing);
    if width <= available {
        size
    } else {
        size * available / width
    }
}

/// Format with thousands separators, replacing JS `toLocaleString`.
pub fn thousands(value: f64) -> String {
    let rounded = value.round().abs() as u64;
    let digits = rounded.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }

    if value < 0.0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Escape text destined for an XML text node or attribute.
///
/// Only the accessible `<title>` carries raw text — everything visible is
/// outlines — but a username reaching a document unescaped is exactly the kind
/// of thing that turns into an injection later.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// A tier's full label, e.g. `Gold II` or `Challenger`.
pub fn tier_label(rank: &RankResult) -> String {
    match rank.division {
        Some(division) => format!("{} {division}", rank.tier),
        None => rank.tier.to_string(),
    }
}

/// Render the card as a standalone SVG document.
pub fn render_card(input: &CardInput<'_>) -> String {
    let palette = palette(input.theme);
    let (gradient, accent) = tier_colors(input.rank.tier);
    // Namespaced so several cards can coexist in one document.
    let ns = card_namespace(input);

    let mut svg = String::with_capacity(8 * 1024);
    let label = tier_label(input.rank);

    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{CARD_WIDTH}" height="{CARD_HEIGHT}" viewBox="0 0 {CARD_WIDTH} {CARD_HEIGHT}" role="img" aria-labelledby="{ns}title">"#
    ));
    svg.push_str(&format!(
        r#"<title id="{ns}title">{} is ranked {label} with a rating of {} on GitHub Ranked</title>"#,
        escape(input.username),
        thousands(input.rank.elo as f64),
    ));

    svg.push_str(&defs(&ns, &palette, gradient));
    svg.push_str(&background(&ns, &palette));
    svg.push_str(&header(input, &palette, gradient[0]));
    svg.push_str(&tier_icon(input.rank.tier, &ns, PADDING, main_top() + (main_height() - ICON_SIZE) / 2.0, ICON_SIZE));
    // Tier accents are tuned for dark surfaces; swap in a darker stop where
    // the theme would otherwise render the label illegible.
    let label_color = readable_tier_color(palette.background_primary, accent, gradient);
    svg.push_str(&rank_block(input, &palette, &label_color, &ns, &label));
    svg.push_str(&stats_panel(input, &palette));
    svg.push_str("</svg>");
    svg
}

fn main_top() -> f64 {
    PADDING + HEADER_HEIGHT + HEADER_GAP
}

fn main_height() -> f64 {
    CARD_HEIGHT - PADDING - main_top()
}

fn defs(ns: &str, palette: &Palette, gradient: [&str; 2]) -> String {
    format!(
        r##"<defs><linearGradient id="{ns}bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="{p}"/><stop offset="50%" stop-color="{s}"/><stop offset="100%" stop-color="{p}"/></linearGradient><linearGradient id="{ns}bar" x1="0" y1="0" x2="1" y2="0"><stop offset="0%" stop-color="{g0}"/><stop offset="100%" stop-color="{g1}"/></linearGradient></defs>"##,
        p = palette.background_primary,
        s = palette.background_secondary,
        g0 = gradient[0],
        g1 = gradient[1],
    )
}

fn background(ns: &str, palette: &Palette) -> String {
    // `minimal` is deliberately transparent so it sits on any README background.
    let fill = if palette.is_transparent() {
        "none".to_string()
    } else {
        format!("url(#{ns}bg)")
    };

    format!(
        r#"<rect x="0.5" y="0.5" width="{w}" height="{h}" rx="{CORNER_RADIUS}" fill="{fill}" stroke="{border}" stroke-width="1"/>"#,
        w = CARD_WIDTH - 1.0,
        h = CARD_HEIGHT - 1.0,
        border = palette.border,
    )
}

fn header(input: &CardInput<'_>, palette: &Palette, tier_color: &str) -> String {
    let center = PADDING + HEADER_HEIGHT / 2.0;
    let mut out = String::new();

    const BRAND: &str = "GITHUB RANKED";
    const BRAND_SIZE: f64 = 11.0;
    const BRAND_SPACING: f64 = 0.05;

    out.push_str(&text_svg(
        BRAND, PADDING, baseline(center, BRAND_SIZE), BRAND_SIZE,
        Weight::Regular, palette.text_secondary, BRAND_SPACING, Anchor::Start,
    ));

    // Season pill, sitting just after the wordmark.
    let season = input.season.unwrap_or(input.current_year);
    let season_label = format!("S{season}");
    let season_size = 10.0;
    let text_width = measure(&season_label, season_size, Weight::Bold, 0.0);
    let pill_x = PADDING + measure(BRAND, BRAND_SIZE, Weight::Regular, BRAND_SPACING) + 8.0;
    let pill_width = text_width + 12.0;
    let pill_height = 15.0;

    out.push_str(&format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{pill_height}" rx="4" fill="{tier_color}" fill-opacity="0.15"/>"#,
        x = round(pill_x),
        y = round(center - pill_height / 2.0),
        w = round(pill_width),
    ));
    // The pill sits on a 15%-tint of the tier colour over the card, so measure
    // against the card itself — close enough, and it keeps the label readable.
    let pill_text = ensure_contrast(palette.background_primary, tier_color, CONTRAST_BODY);
    out.push_str(&text_svg(
        &season_label, pill_x + 6.0, baseline(center, season_size), season_size,
        Weight::Bold, &pill_text, 0.0, Anchor::Start,
    ));

    // Handle, right-aligned.
    let handle = format!("@{}", input.username);
    let handle_size = fit_size(&handle, 13.0, Weight::Bold, 0.0, 160.0);
    out.push_str(&text_svg(
        &handle, CARD_WIDTH - PADDING, baseline(center, handle_size), handle_size,
        Weight::Bold, palette.text_primary, 0.0, Anchor::End,
    ));

    out
}

fn rank_block(
    input: &CardInput<'_>,
    palette: &Palette,
    accent: &str,
    ns: &str,
    label: &str,
) -> String {
    let x = PADDING + ICON_SIZE + COLUMN_GAP;
    let available = CARD_WIDTH - PADDING - PANEL_WIDTH - COLUMN_GAP - x;

    // Stack: tier name, rating line, progress bar — centred in the main area.
    const TIER_SIZE: f64 = 28.0;
    const RATING_SIZE: f64 = 22.0;
    const GAP: f64 = 6.0;

    let stack_height = TIER_SIZE + GAP + RATING_SIZE + GAP + PROGRESS_HEIGHT;
    let top = main_top() + (main_height() - stack_height) / 2.0;

    let mut out = String::new();

    // Long names like GRANDMASTER get scaled down rather than overflowing.
    let upper = label.to_uppercase();
    let tier_size = fit_size(&upper, TIER_SIZE, Weight::Bold, -0.02, available);
    out.push_str(&text_svg(
        &upper, x, baseline(top + TIER_SIZE / 2.0, tier_size), tier_size,
        Weight::Bold, accent, -0.02, Anchor::Start,
    ));

    // Rating, with its unit label on the same baseline.
    let rating_center = top + TIER_SIZE + GAP + RATING_SIZE / 2.0;
    let elo = thousands(input.rank.elo as f64);
    out.push_str(&text_svg(
        &elo, x, baseline(rating_center, RATING_SIZE), RATING_SIZE,
        Weight::Bold, palette.text_primary, 0.0, Anchor::Start,
    ));
    out.push_str(&text_svg(
        "Rating",
        x + measure(&elo, RATING_SIZE, Weight::Bold, 0.0) + 6.0,
        baseline(rating_center, RATING_SIZE),
        13.0,
        Weight::Regular,
        palette.text_secondary,
        0.0,
        Anchor::Start,
    ));

    // Progress through the division. Undivided tiers have no GP, so the bar is
    // shown full rather than empty — Master+ is not "0% of the way" anywhere.
    let bar_y = top + stack_height - PROGRESS_HEIGHT;
    let bar_width = available.min(200.0);
    let progress = if input.rank.tier.has_divisions() {
        (input.rank.gp as f64 / 100.0).clamp(0.0, 1.0)
    } else {
        1.0
    };

    out.push_str(&format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{PROGRESS_HEIGHT}" rx="3" fill="{track}" fill-opacity="0.35"/>"#,
        x = round(x), y = round(bar_y), w = round(bar_width),
        track = palette.border,
    ));
    if progress > 0.0 {
        out.push_str(&format!(
            r#"<rect x="{x}" y="{y}" width="{w}" height="{PROGRESS_HEIGHT}" rx="3" fill="url(#{ns}bar)"/>"#,
            x = round(x), y = round(bar_y), w = round(bar_width * progress),
        ));
    }

    out
}

fn stats_panel(input: &CardInput<'_>, palette: &Palette) -> String {
    let x = CARD_WIDTH - PADDING - PANEL_WIDTH;
    let y = main_top();
    let height = main_height();

    let mut out = format!(
        r#"<rect x="{x}" y="{y}" width="{PANEL_WIDTH}" height="{h}" rx="10" fill="{fill}" fill-opacity="0.5" stroke="{stroke}" stroke-opacity="0.25" stroke-width="1"/>"#,
        h = round(height),
        fill = palette.background_secondary,
        stroke = palette.border,
    );

    let rows = [
        ("PRs", input.stats.total_merged_prs),
        ("Reviews", input.stats.total_code_reviews),
        ("Commits", input.stats.total_commits),
        ("Stars", input.stats.total_stars),
    ];

    let content_height = 4.0 * STAT_ROW_HEIGHT + 3.0 * STAT_ROW_GAP;
    let first_center = y + (height - content_height) / 2.0 + STAT_ROW_HEIGHT / 2.0;

    for (index, (label, value)) in rows.iter().enumerate() {
        let center = first_center + index as f64 * (STAT_ROW_HEIGHT + STAT_ROW_GAP);

        out.push_str(&text_svg(
            label, x + PANEL_PADDING, baseline(center, 12.0), 12.0,
            Weight::Regular, palette.text_secondary, 0.0, Anchor::Start,
        ));
        // Right-aligned so any magnitude fits without reflowing the panel.
        // Stat accents are tuned for dark cards; darken them on light themes
        // rather than shipping 2.2:1 green on white.
        let value_color = ensure_contrast(
            palette.background_primary,
            STAT_COLORS[index],
            CONTRAST_BODY,
        );
        out.push_str(&text_svg(
            &thousands(*value),
            x + PANEL_WIDTH - PANEL_PADDING,
            baseline(center, 13.0),
            13.0,
            Weight::Bold,
            &value_color,
            0.0,
            Anchor::End,
        ));
    }

    out
}

fn round(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ranking::{calculate_rank, Division, Tier};

    fn sample() -> (RankResult, AggregatedStats) {
        let stats = AggregatedStats {
            total_merged_prs: 342.0,
            total_code_reviews: 1287.0,
            total_issues_closed: 96.0,
            total_commits: 4521.0,
            total_stars: 12480.0,
            ..Default::default()
        };
        (calculate_rank(&stats), stats)
    }

    fn render(theme: Theme) -> String {
        let (rank, stats) = sample();
        render_card(&CardInput {
            username: "octocat",
            rank: &rank,
            stats: &stats,
            theme,
            season: None,
            current_year: 2026,
        })
    }

    #[test]
    fn thousands_separators_match_locale_formatting() {
        assert_eq!(thousands(0.0), "0");
        assert_eq!(thousands(999.0), "999");
        assert_eq!(thousands(1_000.0), "1,000");
        assert_eq!(thousands(12_480.0), "12,480");
        assert_eq!(thousands(1_234_567.0), "1,234,567");
        assert_eq!(thousands(-4_521.0), "-4,521");
    }

    #[test]
    fn renders_a_well_formed_document() {
        let svg = render(Theme::Default);

        assert!(svg.starts_with("<svg xmlns="));
        assert!(svg.ends_with("</svg>"));
        assert_eq!(svg.matches("<svg").count(), 1);
        assert_eq!(svg.matches("</svg>").count(), 1);
        assert_eq!(svg.matches("<g ").count(), svg.matches("</g>").count());
        assert_eq!(svg.matches("<defs>").count(), svg.matches("</defs>").count());
    }

    #[test]
    fn every_theme_renders() {
        for theme in Theme::ALL {
            let svg = render(theme);
            assert!(svg.len() > 2_000, "{theme} produced a suspiciously small card");
            assert!(!svg.contains("{NS}"), "{theme} leaked a placeholder");
        }
    }

    #[test]
    fn minimal_theme_paints_no_background() {
        assert!(render(Theme::Minimal).contains(r#"fill="none""#));
        assert!(!render(Theme::Default).contains(r#"fill="none""#));
    }

    #[test]
    fn every_tier_renders_including_undivided_ones() {
        for tier in Tier::ALL_DESC {
            let rank = RankResult {
                tier,
                division: tier.has_divisions().then_some(Division::II),
                elo: 1500,
                gp: 50,
                percentile: 75.0,
                wpi: 1000.0,
                z_score: 0.5,
            };
            let stats = AggregatedStats::default();
            let svg = render_card(&CardInput {
                username: "octocat", rank: &rank, stats: &stats,
                theme: Theme::Default, season: None, current_year: 2026,
            });
            assert!(svg.contains("</svg>"), "{tier} failed to render");
        }
    }

    #[test]
    fn undivided_tiers_show_a_full_bar_not_an_empty_one() {
        let master = RankResult {
            tier: Tier::Master, division: None, elo: 2500, gp: 0,
            percentile: 99.0, wpi: 1.0, z_score: 3.0,
        };
        let stats = AggregatedStats::default();
        let svg = render_card(&CardInput {
            username: "octocat", rank: &master, stats: &stats,
            theme: Theme::Default, season: None, current_year: 2026,
        });
        // The filled bar is drawn at the same width as the track.
        assert_eq!(svg.matches(r#"height="6" rx="3""#).count(), 2);
    }

    #[test]
    fn long_tier_names_are_scaled_to_fit() {
        let available = CARD_WIDTH - PADDING - PANEL_WIDTH - COLUMN_GAP
            - (PADDING + ICON_SIZE + COLUMN_GAP);

        for label in ["GRANDMASTER", "CHALLENGER", "PLATINUM IV", "IRON IV"] {
            let size = fit_size(label, 28.0, Weight::Bold, -0.02, available);
            let width = measure(label, size, Weight::Bold, -0.02);
            assert!(width <= available + 0.01, "{label} overflows at {size}px");
        }
    }

    #[test]
    fn usernames_are_escaped_in_the_accessible_title() {
        let (rank, stats) = sample();
        let svg = render_card(&CardInput {
            username: "a<b>&\"c", rank: &rank, stats: &stats,
            theme: Theme::Default, season: None, current_year: 2026,
        });

        assert!(svg.contains("a&lt;b&gt;&amp;&quot;c"));
        assert!(!svg.contains("<b>"));
    }

    /// Regression: the namespace was derived from the tier alone, so two cards
    /// of the same tier in different themes collided and the second borrowed the
    /// first's background gradient.
    #[test]
    fn same_tier_different_theme_gets_distinct_gradient_ids() {
        let (rank, stats) = sample();
        let card = |theme| render_card(&CardInput {
            username: "octocat", rank: &rank, stats: &stats,
            theme, season: None, current_year: 2026,
        });

        let ids = |svg: &str| {
            svg.split(r#"<linearGradient id=""#).skip(1)
                .map(|s| s.split('"').next().unwrap().to_string())
                .collect::<Vec<_>>()
        };

        let dark = ids(&card(Theme::Dark));
        let light = ids(&card(Theme::Light));

        assert!(!dark.is_empty());
        for id in &dark {
            assert!(!light.contains(id), "{id} is shared across themes");
        }
    }

    #[test]
    fn identical_cards_produce_identical_ids() {
        let (rank, stats) = sample();
        let card = || render_card(&CardInput {
            username: "octocat", rank: &rank, stats: &stats,
            theme: Theme::Default, season: None, current_year: 2026,
        });
        assert_eq!(card(), card(), "rendering must be deterministic");
    }

    #[test]
    fn different_users_get_distinct_ids() {
        let (rank, stats) = sample();
        let card = |username| render_card(&CardInput {
            username, rank: &rank, stats: &stats,
            theme: Theme::Default, season: None, current_year: 2026,
        });
        let a = card("octocat");
        let b = card("torvalds");
        let first_id = |svg: &str| svg.split(r#"<linearGradient id=""#).nth(1)
            .unwrap().split('"').next().unwrap().to_string();
        assert_ne!(first_id(&a), first_id(&b));
    }

    #[test]
    fn season_label_defaults_to_the_current_year() {
        let (rank, stats) = sample();
        let input = |season| CardInput {
            username: "octocat", rank: &rank, stats: &stats,
            theme: Theme::Default, season, current_year: 2026,
        };
        // Rendered as outlines, so assert via the measured pill instead.
        assert_ne!(render_card(&input(None)), render_card(&input(Some(2024))));
    }
}
