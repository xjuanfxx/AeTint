//! Temas XML de la interfaz anterior a Spectrum (After Effects 2024 y anteriores), dentro de AfterFXLib.dll:
//! "AECOLORTHEMES" (timeline y paneles de AE), "DVACOLORTHEMESV5", "DESIGNAPPCOLORTHEMESV5" y
//! "DVACOLORTHEMESV6" (tokens de Spectrum por brillo).
//!
//! Cada color es un `<KeyFrame name="&…;" h=".." s=".." v=".." a=".." />`, dentro de un `<Theme t="&Darkest;">`
//! o con su propio `t` (posición en el deslizador de brillo, 0 = oscuro, 1 = claro). Los valores pueden ser
//! entidades del DTD del propio archivo. Los grises (s ≈ 0) pasan a la paleta y los azules al acento; el texto
//! se compacta (espacios) para que el recurso conserve su tamaño.

use crate::color::Rgb;
use crate::dvaui::{is_accent, keep_grey};
use crate::remap::{shift_accent, GreyMap, Pal, ToneRef};
use std::collections::HashMap;

/// Recursos XML con colores (solo existen con la interfaz antigua; "AECOLORTHEMES" marca esa interfaz).
/// V5 va primero: sus grises por tema sirven de referencia a los temas que no traen los suyos (V3).
/// V1–V4 son temas más antiguos que AE 2024 sigue usando (listas, el panel Project, Effects & Presets…).
pub const RESOURCES: &[&str] = &[
    "DVACOLORTHEMESV5",
    "DESIGNAPPCOLORTHEMESV5",
    "DVACOLORTHEMESV6",
    "AECOLORTHEMES",
    "DVACOLORTHEMESV4",
    "DVACOLORTHEMESV3",
    "DVACOLORTHEMESV2",
    "DVACOLORTHEMESV1",
];
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

/// Atributos de una etiqueta, en orden. Admite espacios alrededor del "=" (`h = "0"`): AE 2023 y anteriores
/// escriben así varios colores, y sin leerlos se perdían su tono y su saturación (el playhead rojo salía gris).
fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].split_whitespace().last().unwrap_or("").to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(val) = after.strip_prefix('"') else {
            rest = &rest[eq + 1..];
            continue;
        };
        let Some(end) = val.find('"') else { break };
        out.push((key, val[..end].to_string()));
        rest = &val[end + 1..];
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

/// Otro color con nombre propio (familias de Spectrum, curvas del graph editor…): nunca es el acento.
fn own_colour(n: &str) -> bool {
    // familias: palabra completa ("colored" no es "red")
    const F: &[&str] = &["celery", "chartreuse", "yellow", "magenta", "fuchsia", "purple", "indigo", "seafoam", "cyan", "red", "orange", "green", "negative", "positive", "notice"];
    n.split('-').any(|w| F.contains(&w)) || ["graph", "vary"].iter().any(|f| n.contains(f))
}

/// En estos temas el único azul es el acento de Adobe (selección, enlaces, botones principales…).
fn blue(h: f32, s: f32) -> bool {
    (190.0..=235.0).contains(&h) && s > 0.3
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

/// ¿El tema XML sigue con los colores de Adobe? Mira el fondo de la aplicación (`kColor_ApplicationBackground`):
/// en el original es gris en todos los temas; con un tema de color deja de serlo.
pub fn looks_original(orig: &[u8]) -> bool {
    let Ok(s) = std::str::from_utf8(orig) else { return false };
    let ents = entities(s);
    let mut seen = false;
    for k in keyframes(s) {
        if get(&k.attrs, "name") != Some("&kColor_ApplicationBackground;") {
            continue;
        }
        // sin "s" es un gris (el original solo da el brillo, v)
        let sat = get(&k.attrs, "s").and_then(|v| resolve(&ents, v)).unwrap_or(0.0);
        if sat > 0.02 {
            return false;
        }
        seen = true;
    }
    seen
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

/// Nombre comparable entre el JSON de 2025+ y estos temas: "time-ruler-cti-comp" y
/// "kAEColor_TimeRuler_CTI_COMP" dan los dos "timerulercticomp".
fn plain(n: &str) -> String {
    let n: String = n.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    for p in ["kaecolor", "kcolor"] {
        if let Some(x) = n.strip_prefix(p) {
            return x.to_string();
        }
    }
    n
}

/// Recolorea un recurso XML de temas. Devuelve el texto nuevo con la misma longitud en bytes.
/// `shared`: grises de referencia por tema de los recursos ya procesados; se completa con los de este.
/// `overrides`: colores a mano (claves del JSON de 2025+, sin "ae-color-"); solo en los brillos oscuros.
pub fn recolor(orig: &[u8], pal: &Pal, light: Option<&Pal>, shared: &mut HashMap<String, ToneRef>, overrides: &std::collections::BTreeMap<String, String>) -> Result<Vec<u8>, String> {
    let s = std::str::from_utf8(orig).map_err(|_| tr!("XML theme: not UTF-8", "tema XML: no es UTF-8"))?;
    let ents = entities(s);
    let kfs = keyframes(s);
    let hand: HashMap<String, Rgb> = overrides.iter().filter_map(|(k, v)| Some((plain(k), crate::color::parse_hex(v)?))).collect();

    // referencia por brillo: los temas con nombre (V5/V6) usan sus propios grises; los fotogramas con "t"
    // (AECOLORTHEMES) usan los extremos del deslizador: oscuro = fondo 0.0 / paneles 0.15, claro = el de AE
    let mut tones: HashMap<String, ToneRef> = HashMap::new();
    let mut themes: Vec<String> = kfs.iter().filter_map(|k| k.theme.clone()).collect();
    themes.dedup();
    for t in themes {
        let Some(dark) = is_dark_theme(&t) else { continue };
        let group: Vec<&Kf> = kfs.iter().filter(|k| k.theme.as_deref() == Some(t.as_str())).collect();
        if let Some(tone) = tone_of(&group, &ents, dark) {
            shared.entry(t.clone()).or_insert(tone);
            tones.insert(t, tone);
        } else if let Some(tone) = shared.get(&t) {
            tones.insert(t, *tone);
        }
    }
    // con un punto de marcos: los grises un poco más claros que los paneles (la zona de capas de la timeline, 0.2)
    // caen entre paneles y marcos, como en 2025+, en vez de mezclarse con el color del texto
    let slider_dark = ToneRef { base: 0.0, raised: 0.10, panel: 0.15, label: 176.0 / 255.0, heading: None, frame: Some(0.25), accent: ADOBE_BLUE };
    let slider_light = ToneRef { base: 0.85, raised: 0.90, panel: 0.95, label: 80.0 / 255.0, heading: None, frame: Some(0.80), accent: ADOBE_BLUE };

    // Deslizador de brillo (AE 2024): cada color tiene fotogramas en varios puntos y AE mezcla los vecinos, así que
    // dos grises distintos de la mitad oscura darían un tono intermedio que no es de la paleta. Cada mitad usa el
    // valor de su extremo (t mínimo en la oscura, máximo en la clara): el color queda fijo en toda la mitad y el
    // cambio de texto claro/oscuro del punto medio se conserva.
    let mut anchors: HashMap<(String, bool), (f32, usize)> = HashMap::new();
    for (i, k) in kfs.iter().enumerate() {
        let (Some(name), Some(t)) = (get(&k.attrs, "name"), get(&k.attrs, "t").and_then(|t| resolve(&ents, t))) else { continue };
        let dark = t <= 0.5;
        let e = anchors.entry((name.to_string(), dark)).or_insert((t, i));
        if (dark && t < e.0) || (!dark && t > e.0) {
            *e = (t, i);
        }
    }

    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut changed = 0;
    for k in &kfs {
        let Some(name) = get(&k.attrs, "name") else { continue };
        // un fotograma con "t" sigue el deslizador aunque esté dentro de un tema (V1/V2)
        let slider = get(&k.attrs, "t").and_then(|t| resolve(&ents, t));
        let (dark, tone) = match (slider, &k.theme) {
            (Some(t), _) if t <= 0.5 => (true, slider_dark),
            (Some(_), _) => (false, slider_light),
            (None, Some(t)) => match (is_dark_theme(t), tones.get(t)) {
                (Some(d), Some(tone)) => (d, *tone),
                _ => continue,
            },
            (None, None) => continue,
        };
        let p = if dark {
            pal
        } else {
            match light {
                Some(l) => l,
                None => continue,
            }
        };
        // color elegido a mano (playhead, keyframe, capa seleccionada…): manda sobre todo lo demás
        let picked = if dark { hand.get(&plain(name)).copied() } else { None };
        // veladuras (alfa < 1): se dejan, salvo el velo del área de trabajo de la timeline: un gris medio al 15 %
        // que AE pone encima de la zona de capas; con el gris de Adobe la deja grisácea, con la paleta queda en su tono
        let wash = token(name).contains("workareawash");
        if picked.is_none() && !wash && get(&k.attrs, "a").and_then(|x| resolve(&ents, x)).is_some_and(|a| a < 1.0) {
            continue;
        }
        // el color de este fotograma: el del extremo de su mitad del deslizador (si lo hay)
        let src = match slider {
            Some(t) => anchors.get(&(name.to_string(), t <= 0.5)).map_or(k, |&(_, i)| &kfs[i]),
            None => k,
        };
        if picked.is_none() && !wash && src.attrs.iter().any(|(key, _)| key == "a") && get(&src.attrs, "a").and_then(|x| resolve(&ents, x)).is_some_and(|a| a < 1.0) {
            continue;
        }
        let Some(v) = get(&src.attrs, "v").and_then(|v| resolve(&ents, v)) else { continue };
        let sat = get(&src.attrs, "s").and_then(|x| resolve(&ents, x)).unwrap_or(0.0);
        let h = get(&src.attrs, "h").and_then(|x| resolve(&ents, x)).unwrap_or(0.0);
        let n = token(name);
        let new = if let Some(c) = picked {
            c
        } else if wash {
            // el velo del área de trabajo: entre los marcos y el texto; al 15 % sobre los paneles da el mismo tono
            // un poco más claro que pinta AE 2025+ en la zona de capas
            crate::color::mix(p.frame, p.text, 0.25)
        } else if sat < 0.04 {
            if keep_grey(&n) || n.contains("shadow") || n.contains("unused") || n.contains("overlay") {
                continue;
            }
            GreyMap::new(&tone, p).map(v)
        } else if accent_like(&n) || (blue(h, sat) && !own_colour(&n)) {
            shift_accent(hsv_to_rgb(h, sat, v), tone.accent, p.accent)
        } else {
            continue;
        };
        // diagnóstico: AETINT_MARK="clave=#rrggbb,clave=#rrggbb" pinta esas claves (sin "k…Color_") de un color fijo
        let new = match std::env::var("AETINT_MARK").ok().and_then(|m| {
            m.split(',').find_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                (name.trim_start_matches('&').trim_end_matches(';').ends_with(k.trim())).then(|| crate::color::parse_hex(v.trim())).flatten()
            })
        }) {
            Some(c) if dark => c,
            _ => new,
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
    // segundo paso: fuera los comentarios <!-- … --> (también dentro del DTD; no cambian nada)
    if s.len() > len {
        let mut t = String::with_capacity(s.len());
        let mut rest = s.as_str();
        while let Some(a) = rest.find("<!--") {
            t.push_str(&rest[..a]);
            match rest[a..].find("-->") {
                Some(e) => rest = &rest[a + e + 3..],
                None => {
                    rest = &rest[a..];
                    break;
                }
            }
        }
        t.push_str(rest);
        s = t;
    }
    if s.len() > len {
        return Err(tr!("XML theme: the result doesn't fit ({} > {} bytes)", "tema XML: el resultado no cabe ({} > {} bytes)", s.len(), len));
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
        let out = recolor(x.as_bytes(), &pal, None, &mut HashMap::new(), &Default::default()).unwrap();
        assert_eq!(out.len(), x.len());
        let o = String::from_utf8(out).unwrap();
        assert!(o.contains("kColor_ApplicationBackground;\" h="), "{o}");
        assert!(!o.contains("&g1;\" />"), "{o}");
    }
}
