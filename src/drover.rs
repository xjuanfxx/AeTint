//! Temas XML de la interfaz anterior a Spectrum (After Effects 2024 y anteriores), dentro de AfterFXLib.dll:
//! "AECOLORTHEMES" (timeline y paneles de AE), "DVACOLORTHEMESV5", "DESIGNAPPCOLORTHEMESV5" y
//! "DVACOLORTHEMESV6" (tokens de Spectrum por brillo).
//!
//! Cada color es un `<KeyFrame name="&…;" h=".." s=".." v=".." a=".." />`, dentro de un `<Theme t="&Darkest;">`
//! o con su propio `t` (posición en el deslizador de brillo, 0 = oscuro, 1 = claro). Los valores pueden ser
//! entidades del DTD del propio archivo. Los grises (s ≈ 0) pasan a la paleta y los azules al acento; el texto
//! se compacta (espacios) para que el recurso conserve su tamaño.

use crate::color::{self, Rgb};
use crate::dvaui::{is_accent, keep_grey};
use crate::remap::{shift_accent, GreyMap, Pal, ToneRef};
use std::collections::HashMap;

/// Recursos XML con colores (solo existen con la interfaz antigua; "AECOLORTHEMES" marca esa interfaz).
pub const RESOURCES: &[&str] = &["AECOLORTHEMES", "DVACOLORTHEMESV6", "DVACOLORTHEMESV5", "DESIGNAPPCOLORTHEMESV5"];
pub const MARKER: &str = "AECOLORTHEMES";

/// El acento de Adobe en esos temas (azul de Spectrum).
const ADOBE_BLUE: Rgb = [0.0784, 0.4510, 0.9020];

fn entities(s: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while let Some(p) = s[i..].find("<!ENTITY") {
        let a = i + p + 8;
        let Some(e) = s[a..].find('>') else { break };
        let body = &s[a..a + e];
        let mut it = body.split_whitespace();
        if let Some(name) = it.next() {
            if let (Some(q1), Some(q2)) = (body.find('"'), body.rfind('"')) {
                if q2 > q1 {
                    m.insert(name.to_string(), body[q1 + 1..q2].to_string());
                }
            }
        }
        i = a + e;
    }
    m
}

fn resolve(ents: &HashMap<String, String>, v: &str) -> Option<f32> {
    let mut v = v.trim().to_string();
    for _ in 0..4 {
        match v.strip_prefix('&').and_then(|x| x.strip_suffix(';')) {
            Some(n) => v = ents.get(n)?.clone(),
            None => break,
        }
    }
    v.trim().parse().ok()
}

/// Atributos de una etiqueta, en orden.
fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq].split_whitespace().last().unwrap_or("").to_string();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else { break };
        out.push((key, after[..end].to_string()));
        rest = &after[end + 1..];
    }
    out
}

/// Nombre del token en el estilo de dvaui ("SPECTRUM_GLOBAL_COLOR_BLUE_400" → "blue-400").
fn token(name: &str) -> String {
    let n = name.trim_start_matches('&').trim_end_matches(';').to_ascii_lowercase().replace('_', "-");
    for p in ["spectrum-global-color-", "spectrum-alias-", "spectrum-", "kaecolor-", "kcolor-"] {
        if let Some(x) = n.strip_prefix(p) {
            return x.to_string();
        }
    }
    n
}

fn accent_like(n: &str) -> bool {
    is_accent(n) || n == "blue" || n.starts_with("blue-") || n.starts_with("focus") || n.contains("-blue")
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let h = (h.rem_euclid(360.0)) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

fn rgb_to_hsv(c: Rgb) -> (f32, f32, f32) {
    let [r, g, b] = c.map(|x| x.clamp(0.0, 1.0));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn num(x: f32, dec: usize) -> String {
    let s = format!("{x:.dec$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

/// ¿Qué paleta lleva un brillo? None = no se toca ("General").
fn is_dark_theme(t: &str) -> Option<bool> {
    match t {
        "Darkest" | "Dark" | "Middark" => Some(true),
        "Midlight" | "Light" | "Lightest" => Some(false),
        _ => None,
    }
}

struct Kf {
    start: usize,
    end: usize,
    attrs: Vec<(String, String)>,
    /// tema que la contiene (Theme t="&X;"), si hay
    theme: Option<String>,
}

fn keyframes(s: &str) -> Vec<Kf> {
    // zonas de cada <Theme t="&X;"> … </Theme>
    let mut zones: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while let Some(p) = s[i..].find("<Theme ") {
        let a = i + p;
        let Some(e) = s[a..].find('>') else { break };
        let t = attrs(&s[a..a + e]).into_iter().find(|(k, _)| k == "t").map(|(_, v)| v.trim_start_matches('&').trim_end_matches(';').to_string());
        let close = s[a..].find("</Theme>").map(|c| a + c).unwrap_or(s.len());
        if let Some(t) = t {
            zones.push((a, close, t));
        }
        i = a + e;
    }
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = s[i..].find("<KeyFrame") {
        let a = i + p;
        let Some(e) = s[a..].find("/>") else { break };
        let end = a + e + 2;
        let theme = zones.iter().find(|z| z.0 <= a && a < z.1).map(|z| z.2.clone());
        out.push(Kf { start: a, end, attrs: attrs(&s[a + 9..a + e]), theme });
        i = end;
    }
    out
}

fn get<'a>(a: &'a [(String, String)], k: &str) -> Option<&'a str> {
    a.iter().find(|(x, _)| x == k).map(|(_, v)| v.as_str())
}

/// Grises de referencia de un brillo, a partir de los colores de ese archivo.
fn tone_of(kfs: &[&Kf], ents: &HashMap<String, String>, dark: bool) -> Option<ToneRef> {
    let grey = |names: &[&str]| -> Option<f32> {
        kfs.iter().find(|k| get(&k.attrs, "name").is_some_and(|n| names.contains(&n))).and_then(|k| get(&k.attrs, "v")).and_then(|v| resolve(ents, v))
    };
    let label = if dark { 176.0 / 255.0 } else { 80.0 / 255.0 };
    let mk = |base: f32, raised: f32, panel: f32| ToneRef { base, raised, panel, label, heading: None, frame: None, accent: ADOBE_BLUE };
    // DVACOLORTHEMESV6 (escala de grises de Spectrum)
    let g = |n: &str| grey(&[&format!("&SPECTRUM_GLOBAL_COLOR_GRAY_{n};")]);
    if let (Some(g50), Some(g75), Some(g100)) = (g("50"), g("75"), g("100")) {
        return Some(if dark { mk(g50, g75, g100) } else { mk(g("200").unwrap_or(g100), g100, g50) });
    }
    // DVACOLORTHEMESV5 / DESIGNAPPCOLORTHEMESV5: fondo de la aplicación y de los paneles
    if let (Some(app), Some(content)) = (grey(&["&kColor_ApplicationBackground;"]), grey(&["&kColor_ContentBackground;"])) {
        return Some(mk(app, (app + content) / 2.0, content));
    }
    None
}

/// Recolorea un recurso XML de temas. Devuelve el texto nuevo con la misma longitud en bytes.
pub fn recolor(orig: &[u8], pal: &Pal, light: Option<&Pal>) -> Result<Vec<u8>, String> {
    let s = std::str::from_utf8(orig).map_err(|_| "tema XML: no es UTF-8".to_string())?;
    let ents = entities(s);
    let kfs = keyframes(s);

    // referencia por brillo: los temas con nombre (V5/V6) usan sus propios grises; los fotogramas con "t"
    // (AECOLORTHEMES) usan los extremos del deslizador: oscuro = fondo 0.0 / paneles 0.15, claro = el de AE
    let mut tones: HashMap<String, ToneRef> = HashMap::new();
    let mut themes: Vec<String> = kfs.iter().filter_map(|k| k.theme.clone()).collect();
    themes.dedup();
    for t in themes {
        let Some(dark) = is_dark_theme(&t) else { continue };
        let group: Vec<&Kf> = kfs.iter().filter(|k| k.theme.as_deref() == Some(t.as_str())).collect();
        if let Some(tone) = tone_of(&group, &ents, dark) {
            tones.insert(t, tone);
        }
    }
    let slider_dark = ToneRef { base: 0.0, raised: 0.10, panel: 0.15, label: 176.0 / 255.0, heading: None, frame: None, accent: ADOBE_BLUE };
    let slider_light = ToneRef { base: 0.85, raised: 0.90, panel: 0.95, label: 80.0 / 255.0, heading: None, frame: None, accent: ADOBE_BLUE };

    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut changed = 0;
    for k in &kfs {
        let Some(name) = get(&k.attrs, "name") else { continue };
        let (dark, tone) = match &k.theme {
            Some(t) => match (is_dark_theme(t), tones.get(t)) {
                (Some(d), Some(tone)) => (d, *tone),
                _ => continue,
            },
            None => match get(&k.attrs, "t").and_then(|t| resolve(&ents, t)) {
                Some(t) if t <= 0.5 => (true, slider_dark),
                Some(_) => (false, slider_light),
                None => continue,
            },
        };
        let p = if dark {
            pal
        } else {
            match light {
                Some(l) => l,
                None => continue,
            }
        };
        let Some(v) = get(&k.attrs, "v").and_then(|v| resolve(&ents, v)) else { continue };
        let sat = get(&k.attrs, "s").and_then(|x| resolve(&ents, x)).unwrap_or(0.0);
        let h = get(&k.attrs, "h").and_then(|x| resolve(&ents, x)).unwrap_or(0.0);
        // veladuras (alfa < 1): se dejan
        if get(&k.attrs, "a").and_then(|x| resolve(&ents, x)).is_some_and(|a| a < 1.0) {
            continue;
        }
        let n = token(name);
        let new = if sat < 0.04 {
            if keep_grey(&n) || n.contains("shadow") || n.contains("unused") || n.contains("overlay") {
                continue;
            }
            GreyMap::new(&tone, p).map(v)
        } else if accent_like(&n) {
            shift_accent(hsv_to_rgb(h, sat, v), tone.accent, p.accent)
        } else {
            continue;
        };
        let (nh, ns, nv) = rgb_to_hsv(new);
        let mut tag = String::from("<KeyFrame");
        let mut wrote = false;
        for (key, val) in &k.attrs {
            match key.as_str() {
                "h" | "s" | "v" => {
                    if !wrote {
                        tag += &format!(" h=\"{}\" s=\"{}\" v=\"{}\"", num(nh, 1), num(ns, 3), num(nv, 3));
                        wrote = true;
                    }
                }
                _ => tag += &format!(" {key}=\"{val}\""),
            }
        }
        tag += "/>";
        out.push_str(&s[last..k.start]);
        out.push_str(&tag);
        last = k.end;
        changed += 1;
    }
    out.push_str(&s[last..]);
    if changed == 0 {
        return Ok(orig.to_vec());
    }
    fit(out, orig.len())
}

/// Ajusta el texto a `len` bytes: si sobra, compacta los espacios (fuera de las etiquetas da igual cuántos haya);
/// si falta, rellena con espacios al final (después del elemento raíz, permitido en XML).
fn fit(s: String, len: usize) -> Result<Vec<u8>, String> {
    let mut s = s;
    if s.len() > len {
        let mut t = String::with_capacity(s.len());
        let mut ws = String::new();
        for c in s.chars() {
            if c.is_whitespace() {
                ws.push(c);
            } else {
                if !ws.is_empty() {
                    t.push(if ws.contains('\n') { '\n' } else { ' ' });
                    ws.clear();
                }
                t.push(c);
            }
        }
        if !ws.is_empty() {
            t.push('\n');
        }
        s = t;
    }
    if s.len() > len {
        return Err(format!("tema XML: el resultado no cabe ({} > {} bytes)", s.len(), len));
    }
    let mut b = s.into_bytes();
    b.resize(len, b' ');
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_roundtrip() {
        for c in [[0.2, 0.4, 0.9], [1.0, 0.0, 0.0], [0.5, 0.5, 0.5], [0.1, 0.8, 0.3]] {
            let (h, s, v) = rgb_to_hsv(c);
            let d = hsv_to_rgb(h, s, v);
            assert!(c.iter().zip(d).all(|(a, b)| (a - b).abs() < 1e-4), "{c:?} → {d:?}");
        }
    }

    #[test]
    fn recolors_and_keeps_length() {
        let x = "<?xml version=\"1.0\"?>\n<!DOCTYPE T [\n  <!ENTITY Darkest \"0.0\">\n  <!ENTITY kColor_ApplicationBackground \"1\">\n  <!ENTITY kColor_ContentBackground \"2\">\n  <!ENTITY g1 \"0.0863\">\n]>\n<ThemeParameters>\n  <ThemeColors>\n    <Theme t=\"&Darkest;\" >\n      <KeyFrame name=\"&kColor_ApplicationBackground;\"      v=\"&g1;\" />\n      <KeyFrame name=\"&kColor_ContentBackground;\"          v=\"0.1373\" />\n      <KeyFrame name=\"&kColor_Blue;\"   h=\"210\" s=\"0.8\" v=\"0.92\" />\n    </Theme>\n  </ThemeColors>\n</ThemeParameters>\n";
        let pal = crate::remap::Palette::default().resolve().unwrap();
        let out = recolor(x.as_bytes(), &pal, None).unwrap();
        assert_eq!(out.len(), x.len());
        let o = String::from_utf8(out).unwrap();
        assert!(o.contains("kColor_ApplicationBackground;\" h="), "{o}");
        assert!(!o.contains("&g1;\" />"), "{o}");
    }
}
