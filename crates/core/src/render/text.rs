//! Text as vector outlines.
//!
//! Cards draw text with `<path>` rather than `<text>`. A README badge is loaded
//! as an `<img>`, where the browser blocks external font loads and falls back to
//! whatever it has — which would shift every glyph away from the positions we
//! measured. Outlines sidestep the problem completely: no font to load, no
//! fallback, identical rendering everywhere.
//!
//! It also removes the five Google Fonts requests the original made *on every
//! render*, cache hits included.

use super::glyphs::{Glyph, BOLD, DESCENDER, FIRST_CHAR, REGULAR, UNITS_PER_EM};

/// The two weights available. CSS weights collapse onto these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Bold,
}

impl Weight {
    /// Map a CSS numeric weight onto an available face.
    pub fn from_css(weight: u16) -> Self {
        if weight >= 600 {
            Self::Bold
        } else {
            Self::Regular
        }
    }

    fn table(self) -> &'static [Glyph; 95] {
        match self {
            Self::Regular => &REGULAR,
            Self::Bold => &BOLD,
        }
    }
}

/// Where `x` sits relative to the rendered run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// Look up a glyph, substituting `?` for anything outside the embedded set.
///
/// Usernames are alphanumerics and hyphens and every label is ASCII, so this
/// only fires on genuinely unexpected input — better a visible `?` than a
/// silently missing character.
fn glyph(ch: char, weight: Weight) -> &'static Glyph {
    let table = weight.table();
    let index = (ch as u32).checked_sub(FIRST_CHAR as u32).map(|i| i as usize);

    match index.and_then(|i| table.get(i)) {
        Some(glyph) => glyph,
        None => &table[('?' as u32 - FIRST_CHAR as u32) as usize],
    }
}

/// Width of `text` when rendered, in user units.
///
/// `letter_spacing` is in em, matching CSS.
pub fn measure(text: &str, size: f64, weight: Weight, letter_spacing: f64) -> f64 {
    if text.is_empty() {
        return 0.0;
    }

    let advances: f64 = text
        .chars()
        .map(|ch| f64::from(glyph(ch, weight).advance))
        .sum();

    // Spacing applies between glyphs, not after the last one — matching how
    // browsers lay out a run.
    let gaps = (text.chars().count() - 1) as f64;

    advances * size / UNITS_PER_EM + gaps * letter_spacing * size
}

/// How far below the baseline the font descends, at a given size.
pub fn descender(size: f64) -> f64 {
    -DESCENDER * size / UNITS_PER_EM
}

/// Render a run of text as SVG paths sitting on `baseline`.
///
/// Glyphs are grouped under one transform so per-glyph path data stays in
/// integral em units.
pub fn text_svg(
    text: &str,
    x: f64,
    baseline: f64,
    size: f64,
    weight: Weight,
    fill: &str,
    letter_spacing: f64,
    anchor: Anchor,
) -> String {
    if text.is_empty() {
        return String::new();
    }

    let width = measure(text, size, weight, letter_spacing);
    let origin = match anchor {
        Anchor::Start => x,
        Anchor::Middle => x - width / 2.0,
        Anchor::End => x - width,
    };

    let scale = size / UNITS_PER_EM;
    // Letter spacing is applied in the scaled space, so convert it to em units.
    let spacing_em = letter_spacing * UNITS_PER_EM;

    let mut out = format!(
        r#"<g transform="translate({} {}) scale({})" fill="{fill}">"#,
        round(origin),
        round(baseline),
        round6(scale),
    );

    let mut pen = 0.0_f64;
    for ch in text.chars() {
        let glyph = glyph(ch, weight);

        // Whitespace has no outline; just advance the pen.
        if !glyph.path.is_empty() {
            out.push_str(&format!(
                r#"<path transform="translate({} 0)" d="{}"/>"#,
                round(pen),
                glyph.path
            ));
        }

        pen += f64::from(glyph.advance) + spacing_em;
    }

    out.push_str("</g>");
    out
}

/// Trim trailing zeros so coordinates don't bloat the output.
fn round(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

fn round6(value: f64) -> String {
    format!("{:.6}", value)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wider_text_measures_wider() {
        let narrow = measure("l", 20.0, Weight::Regular, 0.0);
        let wide = measure("W", 20.0, Weight::Regular, 0.0);
        assert!(wide > narrow, "W should be wider than l");
    }

    #[test]
    fn measurement_scales_linearly_with_size() {
        let small = measure("Challenger", 10.0, Weight::Regular, 0.0);
        let large = measure("Challenger", 20.0, Weight::Regular, 0.0);
        assert!((large - small * 2.0).abs() < 1e-9);
    }

    #[test]
    fn letter_spacing_only_applies_between_glyphs() {
        let plain = measure("abc", 10.0, Weight::Regular, 0.0);
        let spaced = measure("abc", 10.0, Weight::Regular, 0.1);
        // Three glyphs, two gaps.
        assert!((spaced - plain - 2.0).abs() < 1e-9);

        // A single glyph has no gaps to space.
        let one = measure("a", 10.0, Weight::Regular, 0.0);
        assert_eq!(measure("a", 10.0, Weight::Regular, 0.5), one);
    }

    #[test]
    fn empty_text_renders_nothing() {
        assert_eq!(measure("", 10.0, Weight::Regular, 0.0), 0.0);
        assert_eq!(
            text_svg("", 0.0, 0.0, 10.0, Weight::Regular, "#fff", 0.0, Anchor::Start),
            ""
        );
    }

    #[test]
    fn bold_is_wider_than_regular() {
        let regular = measure("Grandmaster", 20.0, Weight::Regular, 0.0);
        let bold = measure("Grandmaster", 20.0, Weight::Bold, 0.0);
        assert!(bold > regular);
    }

    #[test]
    fn css_weights_map_onto_available_faces() {
        assert_eq!(Weight::from_css(400), Weight::Regular);
        assert_eq!(Weight::from_css(500), Weight::Regular);
        assert_eq!(Weight::from_css(600), Weight::Bold);
        assert_eq!(Weight::from_css(800), Weight::Bold);
    }

    #[test]
    fn anchoring_shifts_the_run_not_its_width() {
        let size = 14.0;
        let width = measure("octocat", size, Weight::Regular, 0.0);

        let start = text_svg("octocat", 100.0, 0.0, size, Weight::Regular, "#fff", 0.0, Anchor::Start);
        let end = text_svg("octocat", 100.0, 0.0, size, Weight::Regular, "#fff", 0.0, Anchor::End);

        assert!(start.contains("translate(100 0)"));
        assert!(end.contains(&format!("translate({} 0)", round(100.0 - width))));
    }

    #[test]
    fn spaces_advance_without_drawing() {
        let svg = text_svg("a b", 0.0, 0.0, 10.0, Weight::Regular, "#fff", 0.0, Anchor::Start);
        // Two visible glyphs, and the space contributes no path.
        assert_eq!(svg.matches("<path").count(), 2);
    }

    #[test]
    fn unknown_characters_fall_back_visibly() {
        // Emoji are outside the embedded set.
        let svg = text_svg("\u{1F600}", 0.0, 0.0, 10.0, Weight::Regular, "#fff", 0.0, Anchor::Start);
        assert!(svg.contains("<path"), "should substitute a visible glyph");
        assert_eq!(
            measure("\u{1F600}", 10.0, Weight::Regular, 0.0),
            measure("?", 10.0, Weight::Regular, 0.0)
        );
    }

    #[test]
    fn output_is_well_formed() {
        let svg = text_svg("Gold II", 10.0, 20.0, 14.0, Weight::Bold, "#fff", 0.0, Anchor::Start);
        assert_eq!(svg.matches("<g ").count(), 1);
        assert_eq!(svg.matches("</g>").count(), 1);
        assert!(svg.starts_with("<g transform="));
        assert!(svg.ends_with("</g>"));
    }
}
