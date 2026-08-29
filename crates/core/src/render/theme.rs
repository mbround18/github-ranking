//! Card colour palettes.
//!
//! Values are transcribed from the upstream `themes.ts` by codegen, not by
//! hand, so no palette drifts during the port.

use crate::validation::Theme;

/// The colours a card is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Gradient start, and the card's base colour.
    pub background_primary: &'static str,
    /// Gradient midpoint, and the stats panel fill.
    pub background_secondary: &'static str,
    pub border: &'static str,
    pub text_primary: &'static str,
    pub text_secondary: &'static str,
    pub text_muted: &'static str,
}

impl Palette {
    /// `minimal` is transparent by design, so the card sits on whatever is
    /// behind it — a README in either GitHub theme.
    pub fn is_transparent(&self) -> bool {
        self.background_primary == "transparent"
    }
}

/// The palette for a theme.
pub const fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Default => Palette {
            background_primary: "#0d1117",
            background_secondary: "#161b22",
            border: "#30363d",
            text_primary: "#ffffff",
            text_secondary: "#8b949e",
            text_muted: "#6e7681",
        },
        Theme::Dark => Palette {
            background_primary: "#000000",
            background_secondary: "#0a0a0a",
            border: "#1a1a1a",
            text_primary: "#ffffff",
            text_secondary: "#8b949e",
            text_muted: "#6e7681",
        },
        Theme::Light => Palette {
            background_primary: "#ffffff",
            background_secondary: "#f6f8fa",
            border: "#d0d7de",
            text_primary: "#0d1117",
            text_secondary: "#57606a",
            text_muted: "#8c959f",
        },
        Theme::Minimal => Palette {
            background_primary: "transparent",
            background_secondary: "transparent",
            border: "#30363d",
            text_primary: "#ffffff",
            text_secondary: "#8b949e",
            text_muted: "#6e7681",
        },
        Theme::Cyberpunk => Palette {
            background_primary: "#0a0a0f",
            background_secondary: "#1a1a2e",
            border: "#ff00ff",
            text_primary: "#00ffff",
            text_secondary: "#ff00ff",
            text_muted: "#8b5cf6",
        },
        Theme::Ocean => Palette {
            background_primary: "#0c1929",
            background_secondary: "#1a365d",
            border: "#2b6cb0",
            text_primary: "#e2e8f0",
            text_secondary: "#90cdf4",
            text_muted: "#63b3ed",
        },
        Theme::Forest => Palette {
            background_primary: "#0d1f0d",
            background_secondary: "#1a3a1a",
            border: "#2f5f2f",
            text_primary: "#d4edda",
            text_secondary: "#68d391",
            text_muted: "#48bb78",
        },
        Theme::Sunset => Palette {
            background_primary: "#1a0a0a",
            background_secondary: "#2d1515",
            border: "#c53030",
            text_primary: "#fed7d7",
            text_secondary: "#fc8181",
            text_muted: "#f56565",
        },
        Theme::Galaxy => Palette {
            background_primary: "#0d0d1a",
            background_secondary: "#1a1a33",
            border: "#6b46c1",
            text_primary: "#e9d8fd",
            text_secondary: "#b794f4",
            text_muted: "#9f7aea",
        },
    }
}

/// Parse `#rrggbb` (or `#rgb`) into linear-ish components.
fn parse_hex(colour: &str) -> Option<(f64, f64, f64)> {
    let hex = colour.strip_prefix('#')?;
    let expand = |c: char| u8::from_str_radix(&c.to_string(), 16).ok().map(|v| v * 17);

    let (r, g, b) = match hex.len() {
        3 => {
            let mut chars = hex.chars();
            (
                expand(chars.next()?)?,
                expand(chars.next()?)?,
                expand(chars.next()?)?,
            )
        }
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        _ => return None,
    };

    Some((
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    ))
}

/// WCAG relative luminance.
pub fn luminance(colour: &str) -> f64 {
    let Some((r, g, b)) = parse_hex(colour) else {
        // `transparent` and anything unparseable: assume a dark surface, which
        // is what every non-minimal theme uses.
        return 0.0;
    };

    let channel = |c: f64| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG contrast ratio between two colours, from 1.0 to 21.0.
pub fn contrast_ratio(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (lighter, darker) = if la > lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Blend a colour toward `target` by `amount` (0.0–1.0).
fn mix(colour: (f64, f64, f64), target: f64, amount: f64) -> String {
    let channel = |c: f64| {
        let blended = c + (target - c) * amount;
        (blended.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(colour.0),
        channel(colour.1),
        channel(colour.2)
    )
}

/// WCAG AA floor for large text (the tier label).
pub const CONTRAST_LARGE: f64 = 3.0;

/// WCAG AA floor for body text (stat values, the season pill).
pub const CONTRAST_BODY: f64 = 4.5;

/// Nudge `colour` away from `background` until it clears `min_contrast`.
///
/// Returns the colour unchanged when it already passes, so palettes that were
/// designed carefully are left exactly as authored.
pub fn ensure_contrast(background: &str, colour: &str, min_contrast: f64) -> String {
    if contrast_ratio(background, colour) >= min_contrast {
        return colour.to_string();
    }

    let Some(base) = parse_hex(colour) else {
        return colour.to_string();
    };

    // Move toward black on a light surface, white on a dark one.
    let target = if luminance(background) > 0.5 {
        0.0
    } else {
        1.0
    };

    for step in 1..=20 {
        let candidate = mix(base, target, f64::from(step) / 20.0);
        if contrast_ratio(background, &candidate) >= min_contrast {
            return candidate;
        }
    }

    colour.to_string()
}

/// Pick a tier colour that is actually legible on this theme's background.
///
/// Tier accents are tuned for dark surfaces and two cases break:
///
/// - Gold's accent is `#FFF4B8`, which on the light theme is pale yellow on
///   white.
/// - Iron is three shades of dark grey, none of which clears the floor against
///   the default dark background.
///
/// So we try the accent, then the gradient stops, and if none of them work we
/// push the accent toward white or black until it does. Adjusting beats
/// substituting a generic colour: the label still reads as *that tier*.
pub fn readable_tier_color(
    background: &str,
    accent: &'static str,
    gradient: [&'static str; 2],
) -> String {
    // Prefer a colour the designer actually chose over a computed one.
    for candidate in [accent, gradient[1], gradient[0]] {
        if contrast_ratio(background, candidate) >= CONTRAST_LARGE {
            return candidate.to_string();
        }
    }

    ensure_contrast(background, accent, CONTRAST_LARGE)
}

/// Accent colours for the four stat rows. These are fixed across themes —
/// they encode which metric is which, not the theme.
pub const STAT_COLORS: [&str; 4] = ["#58a6ff", "#a371f7", "#3fb950", "#f0883e"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_a_palette() {
        for theme in Theme::ALL {
            let p = palette(theme);
            assert!(!p.text_primary.is_empty(), "{theme} has no text colour");
        }
    }

    #[test]
    fn only_minimal_is_transparent() {
        for theme in Theme::ALL {
            assert_eq!(
                palette(theme).is_transparent(),
                theme == Theme::Minimal,
                "{theme} transparency is wrong"
            );
        }
    }

    #[test]
    fn luminance_ranks_colours_correctly() {
        assert!(luminance("#ffffff") > 0.9);
        assert!(luminance("#000000") < 0.01);
        assert!(luminance("#ffffff") > luminance("#808080"));
        assert_eq!(luminance("transparent"), 0.0);
    }

    #[test]
    fn contrast_is_symmetric_and_bounded() {
        assert!((contrast_ratio("#000000", "#ffffff") - 21.0).abs() < 0.01);
        assert!((contrast_ratio("#ffffff", "#000000") - 21.0).abs() < 0.01);
        assert!((contrast_ratio("#123456", "#123456") - 1.0).abs() < 0.01);
    }

    /// Regression: Gold's accent is near-white, so on the light theme the tier
    /// label was pale yellow on white.
    #[test]
    fn tier_labels_stay_legible_on_every_theme() {
        use crate::ranking::Tier;
        use crate::ranking::constants::tier_colors;

        for theme in Theme::ALL {
            let background = palette(theme).background_primary;
            if background == "transparent" {
                continue;
            }

            for tier in Tier::ALL_DESC {
                let (gradient, accent) = tier_colors(tier);
                let chosen = readable_tier_color(background, accent, gradient);
                let ratio = contrast_ratio(background, &chosen);

                assert!(
                    ratio >= 3.0,
                    "{tier} on {theme}: contrast {ratio:.2} is below the AA large-text floor"
                );
            }
        }
    }

    #[test]
    fn a_legible_accent_is_left_alone() {
        // Gold's accent is fine on the default dark background.
        let (gradient, accent) = crate::ranking::constants::tier_colors(crate::ranking::Tier::Gold);
        assert_eq!(readable_tier_color("#0d1117", accent, gradient), accent);
    }

    /// Iron is three shades of dark grey; on a dark card none of them clears
    /// the floor, so the accent has to be lightened.
    #[test]
    fn an_illegible_tier_is_adjusted_not_replaced() {
        let (gradient, accent) = crate::ranking::constants::tier_colors(crate::ranking::Tier::Iron);
        let chosen = readable_tier_color("#0d1117", accent, gradient);

        assert_ne!(chosen, accent, "should have been adjusted");
        assert!(contrast_ratio("#0d1117", &chosen) >= 3.0);
        assert!(
            luminance(&chosen) > luminance(accent),
            "should be lightened"
        );
    }

    #[test]
    fn stat_colours_stay_legible_on_every_theme() {
        for theme in Theme::ALL {
            let background = palette(theme).background_primary;
            if background == "transparent" {
                continue;
            }

            for colour in STAT_COLORS {
                let adjusted = ensure_contrast(background, colour, CONTRAST_BODY);
                let ratio = contrast_ratio(background, &adjusted);
                assert!(ratio >= CONTRAST_BODY, "{colour} on {theme}: {ratio:.2}");
            }
        }
    }

    #[test]
    fn a_colour_that_already_passes_is_untouched() {
        assert_eq!(
            ensure_contrast("#000000", "#ffffff", CONTRAST_BODY),
            "#ffffff"
        );
        assert_eq!(
            ensure_contrast("#ffffff", "#000000", CONTRAST_BODY),
            "#000000"
        );
    }

    #[test]
    fn adjustment_moves_away_from_the_background() {
        // Mid-blue on white is too light for body text and must darken.
        let darkened = ensure_contrast("#ffffff", "#58a6ff", CONTRAST_BODY);
        assert!(luminance(&darkened) < luminance("#58a6ff"));

        // The same colour on black must lighten.
        let lightened = ensure_contrast("#000000", "#00308f", CONTRAST_BODY);
        assert!(luminance(&lightened) > luminance("#00308f"));
    }

    #[test]
    fn colours_are_valid_css() {
        for theme in Theme::ALL {
            let p = palette(theme);
            for colour in [
                p.background_primary,
                p.background_secondary,
                p.border,
                p.text_primary,
                p.text_secondary,
                p.text_muted,
            ] {
                assert!(
                    colour == "transparent"
                        || (colour.starts_with('#') && (colour.len() == 7 || colour.len() == 4)),
                    "{theme}: {colour:?} is not a hex colour"
                );
            }
        }
    }
}
