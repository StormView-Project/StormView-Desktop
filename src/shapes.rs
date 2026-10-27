//! M3 Expressive shape library silhouettes (cookies, sunny, burst, flower, puffy,
//! clover…) generated as SVG path commands in a 100×100 viewbox for Slint `Path`.

use std::f64::consts::TAU;

fn polar(radius: impl Fn(f64) -> f64) -> String {
    const STEPS: usize = 180;
    let mut d = String::with_capacity(STEPS * 14);
    for i in 0..=STEPS {
        let th = i as f64 / STEPS as f64 * TAU;
        let r = 50.0 * radius(th);
        let (x, y) = (50.0 + r * (th - TAU / 4.0).cos(), 50.0 + r * (th - TAU / 4.0).sin());
        d.push_str(&format!("{}{x:.2} {y:.2} ", if i == 0 { "M" } else { "L" }));
    }
    d.push('Z');
    d
}

pub fn path(name: &str) -> String {
    match name {
        "sunny" => polar(|t| 0.9 + 0.1 * (8.0 * t).cos()),
        "very-sunny" => polar(|t| 0.84 + 0.16 * (8.0 * t).cos()),
        "4-sided-cookie" => polar(|t| 0.88 + 0.12 * (4.0 * t).cos()),
        "6-sided-cookie" => polar(|t| 0.91 + 0.09 * (6.0 * t).cos()),
        "7-sided-cookie" => polar(|t| 0.92 + 0.08 * (7.0 * t).cos()),
        "9-sided-cookie" => polar(|t| 0.93 + 0.07 * (9.0 * t).cos()),
        "12-sided-cookie" => polar(|t| 0.94 + 0.06 * (12.0 * t).cos()),
        "flower" => polar(|t| 0.78 + 0.22 * (4.0 * t).cos().abs().powf(0.7)),
        "4-leaf-clover" => polar(|t| 0.72 + 0.28 * (2.0 * t).cos().abs().powf(0.55)),
        "8-leaf-clover" => polar(|t| 0.8 + 0.2 * (4.0 * t).cos().abs().powf(0.6)),
        "burst" => polar(|t| 0.8 + 0.2 * (1.0 - (6.0 * t).sin().abs()).powf(2.2)),
        "soft-burst" => polar(|t| 0.84 + 0.16 * (1.0 - (5.0 * t).sin().abs()).powf(1.6)),
        "puffy" => polar(|t| 0.86 + 0.14 * (3.0 * t).cos().abs().powf(0.45)),
        "pentagon" => polar(|t| 0.94 + 0.06 * (5.0 * t).cos()),
        "gem" => polar(|t| 0.9 + 0.1 * (3.0 * t).cos()),
        _ => polar(|_| 1.0),
    }
}
