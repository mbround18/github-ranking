use github_ranked_core::ranking::{AggregatedStats, Division, RankResult, Tier};
use github_ranked_core::render::card::{render_card, CardInput, CARD_HEIGHT, CARD_WIDTH};
use github_ranked_core::render::theme::{contrast_ratio, palette, readable_tier_color};
use github_ranked_core::ranking::constants::tier_colors;
use github_ranked_core::validation::Theme;

fn main() {
    let themes = [Theme::Light, Theme::Sunset];
    let cells: Vec<(Tier, Theme)> = themes.iter()
        .flat_map(|t| Tier::ALL_DESC.iter().map(move |tier| (*tier, *t)))
        .collect();

    let stats = AggregatedStats {
        total_merged_prs: 128.0, total_code_reviews: 340.0, total_commits: 2100.0,
        total_stars: 4820.0, total_issues_closed: 55.0, ..Default::default() };

    let pad = 12.0; let cols = 4.0;
    let rows_n = (cells.len() as f64 / cols).ceil();
    let w = cols * CARD_WIDTH + (cols + 1.0) * pad;
    let h = rows_n * CARD_HEIGHT + (rows_n + 1.0) * pad;
    let mut sheet = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><rect width="{w}" height="{h}" fill="#21262d"/>"##);

    let mut worst = f64::MAX;
    for (i, (tier, theme)) in cells.iter().enumerate() {
        let rank = RankResult {
            tier: *tier,
            division: tier.has_divisions().then_some(Division::II),
            elo: 1847, gp: 62, percentile: 88.0, wpi: 5000.0, z_score: 1.6,
        };
        let bg = palette(*theme).background_primary;
        let (grad, accent) = tier_colors(*tier);
        let ratio = contrast_ratio(bg, &readable_tier_color(bg, accent, grad));
        worst = worst.min(ratio);
        println!("{:<12} {:<8} contrast {ratio:.2}", tier.as_str(), theme.as_str());

        let card = render_card(&CardInput {
            username: "octocat", rank: &rank, stats: &stats,
            theme: *theme, season: None, current_year: 2026 });
        let x = pad + (i as f64 % cols) * (CARD_WIDTH + pad);
        let y = pad + (i as f64 / cols).floor() * (CARD_HEIGHT + pad);
        let inner = &card[card.find('>').unwrap() + 1..card.len() - 6];
        sheet.push_str(&format!(r#"<g transform="translate({x} {y})">{inner}</g>"#));
    }
    sheet.push_str("</svg>");
    println!("worst contrast across all tier/theme pairs: {worst:.2}");

    let tree = resvg::usvg::Tree::from_str(&sheet, &resvg::usvg::Options::default()).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w as u32, h as u32).unwrap();
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    pixmap.save_png("sheet.png").unwrap();
}
