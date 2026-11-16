//! Colores sRGB (0..1 por canal) y mezclas en OKLab, para que los pasos entre tonos se vean parejos.

pub type Rgb = [f32; 3];

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
fn to_lin(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

pub fn parse_hex(h: &str) -> Option<Rgb> {
    let h = h.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([((n >> 16) & 255) as f32 / 255.0, ((n >> 8) & 255) as f32 / 255.0, (n & 255) as f32 / 255.0])
}

pub fn to_hex(c: Rgb) -> String {
    let [r, g, b] = to_u8(c);
    format!("#{r:02X}{g:02X}{b:02X}")
}

pub fn to_u8(c: Rgb) -> [u8; 3] {
    c.map(|x| (clamp01(x) * 255.0).round() as u8)
}

pub fn to_oklab(c: Rgb) -> [f32; 3] {
    let [r, g, b] = c.map(to_lin);
    let l = (0.412_221_47 * r + 0.536_332_54 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn oklab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    ]
}

pub fn from_oklab(lab: [f32; 3]) -> Rgb {
    oklab_to_linear(lab).map(|x| clamp01(to_srgb(clamp01(x))))
}

/// Mezcla a→b (t de 0 a 1) en OKLab.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let (la, lb) = (to_oklab(a), to_oklab(b));
    from_oklab([0, 1, 2].map(|i| la[i] + (lb[i] - la[i]) * t))
}

/// Luminosidad OKLab de un gris (0..1).
pub fn grey_l(v: f32) -> f32 {
    to_oklab([v, v, v])[0]
}

pub fn to_oklch(c: Rgb) -> [f32; 3] {
    let [l, a, b] = to_oklab(c);
    [l, a.hypot(b), b.atan2(a)]
}

/// OKLCh → sRGB; si el color se sale de lo que la pantalla puede mostrar, baja la saturación hasta que entre.
pub fn from_oklch([l, mut ch, h]: [f32; 3]) -> Rgb {
    for _ in 0..24 {
        let lin = oklab_to_linear([l, ch * h.cos(), ch * h.sin()]);
        if lin.iter().all(|&x| (-1e-4..=1.0001).contains(&x)) {
            return lin.map(|x| clamp01(to_srgb(clamp01(x))));
        }
        ch *= 0.85;
    }
    from_oklab([l, 0.0, 0.0])
}

pub fn is_grey(c: Rgb) -> bool {
    (c[0] - c[1]).abs() < 0.012 && (c[1] - c[2]).abs() < 0.012
}

/// Lee "rgb(1, 2, 3)" o "rgba(1, 2, 3, 0.5)". Devuelve color y alfa (None si no tiene).
pub fn parse_css(s: &str) -> Option<(Rgb, Option<String>)> {
    let s = s.trim();
    let inner = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let mut c = [0.0; 3];
    for i in 0..3 {
        c[i] = parts[i].parse::<f32>().ok()? / 255.0;
    }
    Some((c, parts.get(3).map(|a| a.to_string())))
}

pub fn css(c: Rgb, alpha: Option<&str>) -> String {
    let [r, g, b] = to_u8(c);
    match alpha {
        Some(a) => format!("rgba({r}, {g}, {b}, {a})"),
        None => format!("rgb({r}, {g}, {b})"),
    }
}

/// Contraste WCAG entre dos colores.
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let y = |c: Rgb| {
        let [r, g, b] = c.map(to_lin);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (ya, yb) = (y(a), y(b));
    (ya.max(yb) + 0.05) / (ya.min(yb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_roundtrip() {
        assert_eq!(to_hex(parse_hex("#1e1e2e").unwrap()), "#1E1E2E");
    }
    #[test]
    fn oklab_roundtrip() {
        let c = [0.2, 0.5, 0.8];
        let r = from_oklab(to_oklab(c));
        for i in 0..3 {
            assert!((c[i] - r[i]).abs() < 1e-3);
        }
    }
    #[test]
    fn css_parse() {
        let (c, a) = parse_css("rgba(255, 255, 255, 0.5)").unwrap();
        assert_eq!(to_u8(c), [255, 255, 255]);
        assert_eq!(a.as_deref(), Some("0.5"));
        assert!(parse_css("rgb(102, 102, 102)").unwrap().1.is_none());
    }
}
