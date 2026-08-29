//! Tier emblems.
//!
//! Ported from the upstream `RankIcon` components by codegen. Each is drawn in
//! a 64x64 space and scaled by the caller.
//!
//! Gradient ids carry a `{NS}` placeholder that [`tier_icon`] substitutes.
//! SVG ids are document-global, so without this two cards on one page would
//! share the first card's gradients — which is exactly what the frontend
//! gallery does.

use crate::ranking::Tier;

/// The emblem's native coordinate space.
pub const ICON_VIEWBOX: f64 = 64.0;

/// Raw markup for a tier, still containing the `{NS}` placeholder.
const fn markup(tier: Tier) -> &'static str {
    match tier {
        Tier::Iron => {
            r##"<defs><linearGradient id="{NS}iron-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#3a3a3a"/><stop offset="100%" stop-color="#1a1a1a"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}iron-grad)" opacity="0.2"/><polygon points="32 8 52 20 52 44 32 56 12 44 12 20" fill="url(#{NS}iron-grad)"/><circle cx="32" cy="32" r="8" fill="#5c5c5c" opacity="0.8"/>"##
        }
        Tier::Bronze => {
            r##"<defs><linearGradient id="{NS}bronze-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#cd7f32"/><stop offset="100%" stop-color="#8b4513"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}bronze-grad)" opacity="0.2"/><polygon points="32 6 54 16 50 50 32 58 14 50 10 16" fill="url(#{NS}bronze-grad)"/><rect x="28" y="20" width="8" height="24" rx="2" fill="#d4a373"/><rect x="20" y="26" width="24" height="6" rx="2" fill="#d4a373"/>"##
        }
        Tier::Silver => {
            r##"<defs><linearGradient id="{NS}silver-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#c0c0c0"/><stop offset="100%" stop-color="#808080"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}silver-grad)" opacity="0.2"/><polygon points="32 8 50 18 50 44 32 56 14 44 14 18" fill="url(#{NS}silver-grad)"/><polygon points="32 14 38 26 34 26 34 48 30 48 30 26 26 26" fill="#e8e8e8"/>"##
        }
        Tier::Gold => {
            r##"<defs><linearGradient id="{NS}gold-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#FFD700"/><stop offset="100%" stop-color="#FDB931"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}gold-grad)" opacity="0.2"/><polygon points="14 34 20 18 32 26 44 18 50 34 44 50 20 50" fill="url(#{NS}gold-grad)"/><circle cx="32" cy="30" r="6" fill="#fff4b0"/>"##
        }
        Tier::Platinum => {
            r##"<defs><linearGradient id="{NS}plat-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00d4ff"/><stop offset="100%" stop-color="#0099cc"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}plat-grad)" opacity="0.2"/><polygon points="32 6 52 24 40 56 24 56 12 24" fill="url(#{NS}plat-grad)"/><polygon points="32 14 42 28 32 48 22 28" fill="#7fffff" opacity="0.9"/>"##
        }
        Tier::Emerald => {
            r##"<defs><linearGradient id="{NS}emerald-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#50c878"/><stop offset="100%" stop-color="#228b22"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}emerald-grad)" opacity="0.2"/><polygon points="32 8 48 20 52 36 40 52 24 52 12 36 16 20" fill="url(#{NS}emerald-grad)"/><polygon points="32 16 40 26 36 42 28 42 24 26" fill="#90ee90" opacity="0.9"/>"##
        }
        Tier::Diamond => {
            r##"<defs><linearGradient id="{NS}diamond-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#b9f2ff"/><stop offset="100%" stop-color="#00d4ff"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}diamond-grad)" opacity="0.2"/><polygon points="12 24 24 10 40 10 52 24 40 54 24 54" fill="url(#{NS}diamond-grad)"/><polygon points="24 10 32 24 40 10" fill="#e0ffff"/><polygon points="12 24 32 24 24 54" fill="#e0ffff" opacity="0.8"/><polygon points="52 24 32 24 40 54" fill="#e0ffff" opacity="0.8"/>"##
        }
        Tier::Master => {
            r##"<defs><linearGradient id="{NS}master-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#9b59b6"/><stop offset="100%" stop-color="#6a1b9a"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}master-grad)" opacity="0.2"/><circle cx="32" cy="32" r="20" fill="url(#{NS}master-grad)"/><polygon points="32 12 36 26 50 26 38 34 42 48 32 40 22 48 26 34 14 26 28 26" fill="#d4a5ff" opacity="0.9"/>"##
        }
        Tier::Grandmaster => {
            r##"<defs><linearGradient id="{NS}gm-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#e74c3c"/><stop offset="100%" stop-color="#c0392b"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}gm-grad)" opacity="0.2"/><path d="M32 8 C38 16 46 18 48 28 C50 40 42 52 32 56 C22 52 14 40 16 28 C18 18 26 16 32 8 Z" fill="url(#{NS}gm-grad)"/><path d="M32 18 C36 24 40 26 40 32 C40 38 36 44 32 46 C28 44 24 38 24 32 C24 26 28 24 32 18 Z" fill="#ff8a80" opacity="0.9"/>"##
        }
        Tier::Challenger => {
            r##"<defs><linearGradient id="{NS}chall-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#f39c12"/><stop offset="100%" stop-color="#e67e22"/></linearGradient><linearGradient id="{NS}chall-accent" x1="0" y1="0" x2="1" y2="0"><stop offset="0%" stop-color="#ff6b6b"/><stop offset="50%" stop-color="#4ecdc4"/><stop offset="100%" stop-color="#f39c12"/></linearGradient></defs><circle cx="32" cy="32" r="28" fill="url(#{NS}chall-grad)" opacity="0.2"/><polygon points="32 6 54 22 46 54 32 58 18 54 10 22" fill="url(#{NS}chall-grad)"/><polygon points="32 14 38 28 52 28 40 36 44 50 32 42 20 50 24 36 12 28 26 28" fill="url(#{NS}chall-accent)"/>"##
        }
    }
}

/// Render a tier emblem, scaled and positioned, with gradient ids namespaced.
///
/// `ns` must be unique per card within a document.
pub fn tier_icon(tier: Tier, ns: &str, x: f64, y: f64, size: f64) -> String {
    let scale = size / ICON_VIEWBOX;
    format!(
        r#"<g transform="translate({x} {y}) scale({scale})">{body}</g>"#,
        body = markup(tier).replace("{NS}", ns),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tier_has_an_emblem() {
        for tier in Tier::ALL_DESC {
            let svg = tier_icon(tier, "t0-", 0.0, 0.0, 80.0);
            assert!(svg.contains("<g transform"), "{tier} produced no group");
            assert!(svg.len() > 100, "{tier} emblem looks empty");
        }
    }

    #[test]
    fn placeholders_are_always_substituted() {
        for tier in Tier::ALL_DESC {
            let svg = tier_icon(tier, "card1-", 0.0, 0.0, 80.0);
            assert!(
                !svg.contains("{NS}"),
                "{tier} leaked a namespace placeholder"
            );
        }
    }

    #[test]
    fn two_cards_do_not_share_gradient_ids() {
        let first = tier_icon(Tier::Challenger, "a-", 0.0, 0.0, 80.0);
        let second = tier_icon(Tier::Challenger, "b-", 0.0, 0.0, 80.0);

        assert!(first.contains(r#"id="a-chall-grad""#));
        assert!(second.contains(r#"id="b-chall-grad""#));
        // References must be rewritten alongside the definitions.
        assert!(first.contains("url(#a-chall-grad)"));
        assert!(!first.contains("url(#b-chall-grad)"));
    }

    #[test]
    fn jsx_attributes_became_valid_svg() {
        for tier in Tier::ALL_DESC {
            let svg = tier_icon(tier, "t-", 0.0, 0.0, 80.0);
            assert!(!svg.contains("stopColor"), "{tier} kept a JSX attribute");
        }
    }
}
