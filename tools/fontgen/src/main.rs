//! Generates `crates/core/src/render/glyphs.rs`.
//!
//! The renderer draws text as SVG `<path>` data rather than `<text>`, so a card
//! renders identically everywhere — in a README, in any browser, on any OS —
//! with no font loading at all. That means we need glyph outlines at runtime,
//! but embedding whole TTFs would bloat the wasm bundle for the ~95 characters
//! a card can actually contain.
//!
//! So this extracts just those glyphs, normalised to a 1000-unit em and
//! pre-flipped into SVG's y-down coordinate space. The result is a few tens of
//! kilobytes of integer path data instead of megabytes of font.
//!
//! Usage: `cargo run -p fontgen -- <regular.ttf> <bold.ttf> <out.rs>`

use std::fmt::Write as _;

/// Printable ASCII. Usernames are alphanumerics and hyphens, and every label on
/// the card is ASCII, so this is the complete set a card can render.
const FIRST: char = ' ';
const LAST: char = '~';

/// Everything is normalised to this em size, so path data stays integral.
const EM: f64 = 1000.0;

/// Accumulates SVG path commands from a glyph outline, flipping y as it goes.
struct PathBuilder {
    path: String,
    scale: f64,
}

impl PathBuilder {
    fn new(units_per_em: u16) -> Self {
        Self {
            path: String::new(),
            scale: EM / f64::from(units_per_em),
        }
    }

    /// Fonts are y-up, SVG is y-down: negate as we scale so no transform is
    /// needed at render time.
    fn x(&self, v: f32) -> i32 {
        (f64::from(v) * self.scale).round() as i32
    }

    fn y(&self, v: f32) -> i32 {
        (-f64::from(v) * self.scale).round() as i32
    }
}

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.path, "M{} {}", self.x(x), self.y(y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.path, "L{} {}", self.x(x), self.y(y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "Q{} {} {} {}",
            self.x(x1),
            self.y(y1),
            self.x(x),
            self.y(y)
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "C{} {} {} {} {} {}",
            self.x(x1),
            self.y(y1),
            self.x(x2),
            self.y(y2),
            self.x(x),
            self.y(y)
        );
    }

    fn close(&mut self) {
        self.path.push('Z');
    }
}

struct Extracted {
    advance: u16,
    path: String,
}

fn extract(font: &[u8]) -> Result<(Vec<Extracted>, i16, i16), Box<dyn std::error::Error>> {
    let face = ttf_parser::Face::parse(font, 0)?;
    let units = face.units_per_em();
    let scale = EM / f64::from(units);

    let mut glyphs = Vec::new();

    for code in (FIRST as u32)..=(LAST as u32) {
        let ch = char::from_u32(code).expect("ascii range");

        let Some(id) = face.glyph_index(ch) else {
            return Err(format!("font is missing a glyph for {ch:?}").into());
        };

        let advance = face
            .glyph_hor_advance(id)
            .ok_or_else(|| format!("no advance width for {ch:?}"))?;

        let mut builder = PathBuilder::new(units);
        // Whitespace legitimately has no outline; anything else that comes back
        // empty would be a silently blank glyph, so check the advance instead.
        face.outline_glyph(id, &mut builder);

        glyphs.push(Extracted {
            advance: (f64::from(advance) * scale).round() as u16,
            path: builder.path,
        });
    }

    let ascender = (f64::from(face.ascender()) * scale).round() as i16;
    let descender = (f64::from(face.descender()) * scale).round() as i16;

    Ok((glyphs, ascender, descender))
}

fn emit(name: &str, glyphs: &[Extracted], out: &mut String) {
    let _ = writeln!(
        out,
        "/// {} glyphs, indexed by `ch as usize - FIRST as usize`.\npub static {}: [Glyph; {}] = [",
        name.to_lowercase(),
        name,
        glyphs.len()
    );

    for (index, glyph) in glyphs.iter().enumerate() {
        let ch = char::from_u32(FIRST as u32 + index as u32).unwrap();
        let _ = writeln!(
            out,
            "    Glyph {{ advance: {}, path: \"{}\" }}, // {:?}",
            glyph.advance, glyph.path, ch
        );
    }

    out.push_str("];\n\n");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [regular_path, bold_path, out_path] = args.as_slice() else {
        eprintln!("usage: fontgen <regular.ttf> <bold.ttf> <out.rs>");
        std::process::exit(2);
    };

    let (regular, ascender, descender) = extract(&std::fs::read(regular_path)?)?;
    let (bold, _, _) = extract(&std::fs::read(bold_path)?)?;

    let mut out = String::new();
    out.push_str(&format!(
        "//! Glyph outlines for the card renderer. **Generated — do not edit.**\n\
         //!\n\
         //! Regenerate with `./tools/fontgen/regenerate.sh`.\n\
         //!\n\
         //! Source: {regular_path}\n\
         //!         {bold_path}\n\n\
         /// A single glyph: how far to advance after drawing it, and its outline\n\
         /// as SVG path data in a {em}-unit em, already flipped to y-down.\n\
         pub struct Glyph {{\n    pub advance: u16,\n    pub path: &'static str,\n}}\n\n\
         /// Glyph tables start at this character.\n\
         pub const FIRST_CHAR: char = '{first}';\n\n\
         /// Em size every coordinate is normalised to.\n\
         pub const UNITS_PER_EM: f64 = {em:.1};\n\n\
         /// Distance from baseline to the top of the tallest glyph.\n\
         pub const ASCENDER: f64 = {ascender:.1};\n\n\
         /// Distance from baseline to the lowest descender (negative).\n\
         pub const DESCENDER: f64 = {descender:.1};\n\n",
        em = EM,
        first = FIRST,
        ascender = f64::from(ascender),
        descender = f64::from(descender),
    ));

    emit("REGULAR", &regular, &mut out);
    emit("BOLD", &bold, &mut out);

    std::fs::write(out_path, &out)?;

    println!(
        "wrote {out_path}: {} glyphs x2 weights, {:.1} KB",
        regular.len(),
        out.len() as f64 / 1024.0
    );
    Ok(())
}
