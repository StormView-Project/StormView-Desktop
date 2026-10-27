//! Material You color generation with `material-colors` — a Rust port of Google's
//! material-color-utilities. Every color in the app comes from a seed run through
//! the HCT dynamic-color algorithm (spec 2025, the Material 3 Expressive color spec).

use material_colors::{
    blend::harmonize,
    color::Rgb,
    dynamic_color::{dynamic_scheme::DynamicScheme, Platform, SpecVersion},
    hct::Hct,
    scheme::{
        variant::{
            SchemeContent, SchemeExpressive, SchemeFidelity, SchemeFruitSalad, SchemeMonochrome, SchemeNeutral,
            SchemeRainbow, SchemeTonalSpot, SchemeVibrant,
        },
        Scheme,
    },
};

pub const PRESETS: [(&str, &str, u32); 10] = [
    ("storm", "StormView", 0x4f8ef5),
    ("baseline", "Baseline", 0x6750a4),
    ("ocean", "Ocean", 0x0891b2),
    ("sunset", "Sunset", 0xea580c),
    ("forest", "Forest", 0x16a34a),
    ("crimson", "Crimson", 0xe11d48),
    ("lavender", "Lavender", 0x8b5cf6),
    ("sand", "Sand", 0xb08d57),
    ("rose", "Rose", 0xdb2777),
    ("mint", "Mint", 0x10b981),
];

pub const VARIANTS: [(&str, &str, &str); 9] = [
    ("tonal-spot", "Tonal Spot", "The Material You default — calm, low-chroma accents."),
    ("expressive", "Expressive", "Playful hue rotations for a bolder, livelier palette."),
    ("vibrant", "Vibrant", "Maximum chroma for primary colors."),
    ("fidelity", "Fidelity", "Stays as close to the seed color as possible."),
    ("content", "Content", "Matches the seed like Fidelity, with analogous accents."),
    ("rainbow", "Rainbow", "Colorful accents with neutral surfaces."),
    ("fruit-salad", "Fruit Salad", "A playful mix of complementary hues."),
    ("neutral", "Neutral", "Nearly grayscale with a hint of the seed."),
    ("monochrome", "Monochrome", "Pure grayscale."),
];

pub fn parse_hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim().trim_start_matches('#'), 16).ok()
}

pub fn build_scheme(seed: u32, variant: &str, dark: bool, contrast: f64) -> Scheme {
    let hct = Hct::new(Rgb::from_u32(seed));
    let c = Some(contrast);
    let (spec, p) = (SpecVersion::Spec2025, Platform::Phone);
    let dynamic: DynamicScheme = match variant {
        "expressive" => SchemeExpressive::with_spec(hct, dark, c, spec, p).scheme,
        "vibrant" => SchemeVibrant::with_spec(hct, dark, c, spec, p).scheme,
        "fidelity" => SchemeFidelity::with_spec(hct, dark, c, spec, p).scheme,
        "content" => SchemeContent::with_spec(hct, dark, c, spec, p).scheme,
        "rainbow" => SchemeRainbow::with_spec(hct, dark, c, spec, p).scheme,
        "fruit-salad" => SchemeFruitSalad::with_spec(hct, dark, c, spec, p).scheme,
        "neutral" => SchemeNeutral::with_spec(hct, dark, c, spec, p).scheme,
        "monochrome" => SchemeMonochrome::with_spec(hct, dark, c, spec, p).scheme,
        _ => SchemeTonalSpot::with_spec(hct, dark, c, spec, p).scheme,
    };
    Scheme::from(dynamic)
}

pub fn contrast_level(s: &str) -> f64 {
    match s {
        "medium" => 0.5,
        "high" => 1.0,
        _ => 0.0,
    }
}

pub fn rgb(c: Rgb) -> slint::Color {
    slint::Color::from_rgb_u8(c.red, c.green, c.blue)
}

pub fn rgb_u32(c: Rgb) -> u32 {
    ((c.red as u32) << 16) | ((c.green as u32) << 8) | c.blue as u32
}

pub fn color_u32(v: u32) -> slint::Color {
    slint::Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A semantic color (UV level, AQI, alert severity, sun/rain accents) keeps its
/// meaning but is harmonized toward the theme's primary and re-toned for the
/// current scheme — how Material You treats custom colors.
#[derive(Clone, Copy)]
pub struct Semantic {
    pub color: slint::Color,
    pub container: slint::Color,
    pub on_container: slint::Color,
}

pub fn semantic(hex: u32, primary: Rgb, dark: bool) -> Semantic {
    let h = Hct::new(harmonize(Rgb::from_u32(hex), primary));
    // Accents stay vivid; containers are calmer, like M3 tonal containers.
    let accent = h.get_chroma().max(36.0);
    let calm = h.get_chroma().min(30.0);
    let tone = |c: f64, t: f64| rgb(Rgb::from(Hct::from(h.get_hue(), c, t)));
    if dark {
        Semantic { color: tone(accent, 80.0), container: tone(calm, 30.0), on_container: tone(calm, 90.0) }
    } else {
        Semantic { color: tone(accent, 40.0), container: tone(calm, 90.0), on_container: tone(calm, 10.0) }
    }
}

/// Hue-only harmonization for data colors whose tone carries meaning (map polygons, AQI ramp).
pub fn harmonize_u32(hex: u32, primary: Rgb) -> u32 {
    rgb_u32(harmonize(Rgb::from_u32(hex), primary))
}

/// Primary / secondary-container / tertiary-container preview for a seed swatch.
pub fn swatch(seed: u32, dark: bool) -> [slint::Color; 4] {
    let s = build_scheme(seed, "tonal-spot", dark, 0.0);
    [rgb(s.primary), rgb(s.secondary_container), rgb(s.tertiary_container), rgb(s.surface_container_high)]
}

/// Seed that follows the weather for the "Weather" color source.
pub fn adaptive_seed(code: i64, is_day: bool) -> u32 {
    if !is_day && code <= 3 {
        return 0x5b5bd6;
    }
    crate::present::visual(code, is_day).hue
}

/// Swatch preview rendered in Rust: primary top half, secondary/tertiary bottom quarters.
/// `px` is the image size; drawn at 2× for crisp edges on HiDPI screens.
pub fn swatch_image(seed: u32, dark: bool, px: u32) -> slint::Image {
    // All three slices at the same tone (80), like Android's wallpaper color picker,
    // so no single dark slice makes the circle look split on a diagonal.
    let accents = build_scheme(seed, "tonal-spot", true, 0.0);
    let s = build_scheme(seed, "tonal-spot", dark, 0.0);
    let mut pm = tiny_skia::Pixmap::new(px, px).expect("pixmap");
    let c0 = px as f32 / 2.0;
    // Inner pie radius: 20/28 of the swatch, leaving a surface-colored ring.
    let r = c0 * 20.0 / 28.0;
    let paint = |c: Rgb| {
        let mut p = tiny_skia::Paint::default();
        p.anti_alias = true;
        p.set_color_rgba8(c.red, c.green, c.blue, 255);
        p
    };
    let slice = |start: f32, sweep: f32| {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(c0, c0);
        let steps = 48;
        for i in 0..=steps {
            let a = (start + sweep * i as f32 / steps as f32).to_radians();
            pb.line_to(c0 + r * a.cos(), c0 + r * a.sin());
        }
        pb.close();
        pb.finish().expect("path")
    };
    let id = tiny_skia::Transform::identity();
    let rule = tiny_skia::FillRule::Winding;
    let ring = tiny_skia::PathBuilder::from_circle(c0, c0, c0).expect("circle");
    pm.fill_path(&ring, &paint(s.surface_container_high), rule, id, None);
    pm.fill_path(&slice(180.0, 180.0), &paint(accents.primary), rule, id, None);
    pm.fill_path(&slice(90.0, 90.0), &paint(accents.secondary), rule, id, None);
    pm.fill_path(&slice(0.0, 90.0), &paint(accents.tertiary), rule, id, None);
    let mut data = pm.take();
    for c in data.chunks_exact_mut(4) {
        let a = c[3] as u32;
        if a > 0 && a < 255 {
            for v in &mut c[..3] {
                *v = ((*v as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(&data, px, px))
}
