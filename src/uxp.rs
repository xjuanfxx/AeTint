//! Pantalla de inicio (Home) de After Effects: plugin UXP de Adobe `com.adobe.ccx.start`, hecho con hojas de
//! estilo Spectrum. Cada regla dice para qué brillo es (`.spectrum--darkest …`, `.spectrum--dark …`,
//! `.spectrum--light …`): las oscuras toman la paleta oscura y las claras la clara (si hay).
//!
//! Los archivos vienen en dos formatos: CSS normal (también dentro de los .js) y una versión ya analizada en
//! JSON (`{"selectorText": "…", "style": {…}}`).

use crate::color::{self, Rgb};
use crate::remap::{shift_accent, GreyMap, Pal, ToneRef};

/// Carpeta del plugin dentro de "Support Files".
pub const HOME_DIR: &str = "UXP/plugins/com.adobe.ccx.start";

/// Archivos con estilos de la pantalla de inicio (relativos a "Support Files"). Los que no existan se saltan.
pub const HOME_FILES: &[&str] = &[
    "UXP/plugins/com.adobe.ccx.start/css/styles.css",
    "UXP/plugins/com.adobe.ccx.start/css/app.css",
    "UXP/plugins/com.adobe.ccx.start/js/spectrum.css",
    "UXP/plugins/com.adobe.ccx.start/js/desktop.css",
    "UXP/plugins/com.adobe.ccx.start/js/5.js",
    "UXP/plugins/com.adobe.ccx.start/js/67.bundle.js",
    // AE 2024: el CSS de Spectrum va incrustado aquí
    "UXP/plugins/com.adobe.ccx.start/js/4.js",
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Theme {
    Darkest,
    Dark,
    Light,
}

/// ¿Para qué brillo es este selector? Si mezcla brillos claros y oscuros, no se toca.
fn theme_of(sel: &str) -> Option<Theme> {
    let s = sel.to_ascii_lowercase();
    let light = ["spectrum--light", "theme-light"].iter().any(|m| s.contains(m));
    let darkest = s.matches("spectrum--darkest").count() + s.matches("theme-darkest").count();
    let dark = s.matches("spectrum--dark").count() + s.matches("theme-dark").count() - darkest;
    match (light, darkest > 0, dark > 0) {
        (true, false, false) => Some(Theme::Light),
        (false, true, false) => Some(Theme::Darkest),
        (false, false, true) => Some(Theme::Dark),
        _ => None,
    }
}

struct Mapper {
    darkest: (GreyMap, Rgb),
    dark: (GreyMap, Rgb),
    accent: Rgb,
    /// modo claro: mapa, acento original y acento nuevo (None: el claro queda como el de Adobe)
    light: Option<(GreyMap, Rgb, Rgb)>,
}

impl Mapper {
    fn color(&self, t: Theme, c: Rgb) -> Option<Rgb> {
        let (map, orig_acc, new_acc) = match t {
            Theme::Darkest => (&self.darkest.0, self.darkest.1, self.accent),
            Theme::Dark => (&self.dark.0, self.dark.1, self.accent),
            Theme::Light => {
                let l = self.light.as_ref()?;
                (&l.0, l.1, l.2)
            }
        };
        if color::is_grey(c) {
            return Some(map.map((c[0] + c[1] + c[2]) / 3.0));
        }
        // azules de Spectrum (acento): mismo tono que el acento original y con color de sobra
        let k = color::to_oklch(c);
        let o = color::to_oklch(orig_acc);
        let dh = ((k[2] - o[2]).abs() % 360.0).min(360.0 - (k[2] - o[2]).abs() % 360.0);
        if k[1] > 0.06 && dh < 22.0 {
            return Some(shift_accent(c, orig_acc, new_acc));
        }
        None
    }
}

fn hex_val(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|v| v as u8)
}

/// Cambia los colores (#hex, rgb(), rgba()) de un bloque de declaraciones.
fn recolor(body: &str, t: Theme, m: &Mapper) -> String {
    let b = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        // #rgb, #rrggbb, #rrggbbaa (opacos solo)
        if b[i] == b'#' && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let mut n = 0;
            while i + 1 + n < b.len() && hex_val(b[i + 1 + n]).is_some() {
                n += 1;
            }
            let end_ok = i + 1 + n >= b.len() || !b[i + 1 + n].is_ascii_alphanumeric();
            if end_ok && (n == 3 || n == 6) {
                let h = &body[i + 1..i + 1 + n];
                let full = if n == 3 { h.chars().flat_map(|c| [c, c]).collect::<String>() } else { h.to_string() };
                if let Some(c) = color::parse_hex(&format!("#{full}")) {
                    if let Some(nc) = m.color(t, c) {
                        out.push_str(&body[last..i]);
                        out.push_str(&color::to_hex(nc).to_ascii_lowercase());
                        i += 1 + n;
                        last = i;
                        continue;
                    }
                }
            }
            i += 1 + n.max(0);
            continue;
        }
        // rgb(...) / rgba(...)
        if (body[i..].starts_with("rgb(") || body[i..].starts_with("rgba(")) && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            if let Some(close) = body[i..].find(')') {
                let lit = &body[i..i + close + 1];
                if let Some((c, alpha)) = color::parse_css(lit) {
                    let opaque = alpha.as_deref().map_or(true, |a| a.parse::<f32>().map_or(false, |x| x >= 1.0));
                    // los grises translúcidos son veladuras y sombras: se dejan
                    if opaque || !color::is_grey(c) {
                        if let Some(nc) = m.color(t, c) {
                            out.push_str(&body[last..i]);
                            out.push_str(&color::css(nc, alpha.as_deref()));
                            i += close + 1;
                            last = i;
                            continue;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    out.push_str(&body[last..]);
    out
}

/// Variables de grises oscuros definidas en `:root` (fuera de las reglas de brillo): la escala Spectrum de
/// cada brillo, de donde la Home saca su fondo, y los grises propios de la Home (cabecera, tarjetas…).
const DARK_VARS: &[(&str, Theme)] = &[
    ("--spectrum-global-color-darkest-gray-", Theme::Darkest),
    ("--spectrum-global-color-dark-gray-", Theme::Dark),
    ("--color-dark-grey-", Theme::Darkest),
    ("--spectrum-global-color-light-gray-", Theme::Light),
    ("--spectrum-global-color-lightest-gray-", Theme::Light),
    ("--color-grey-", Theme::Light),
];

/// Colores de una regla: todos si es de un brillo oscuro; si no, solo las variables de grises oscuros.
fn restyle(sel: &str, body: &str, m: &Mapper) -> String {
    if let Some(t) = theme_of(sel) {
        return recolor(body, t, m);
    }
    if !DARK_VARS.iter().any(|(v, _)| body.contains(v)) {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    let mut spans: Vec<(usize, usize, Theme)> = Vec::new();
    for &(v, theme) in DARK_VARS {
        let mut from = 0;
        while let Some(k) = body[from..].find(v).map(|x| from + x) {
            // solo definiciones: `--nombre-3:` o `"--nombre-3":` (no los usos `var(--nombre-3)`)
            let digits = body[k + v.len()..].chars().take_while(|c| c.is_ascii_digit()).count();
            let after = &body[k + v.len() + digits..];
            let colon = if after.starts_with(':') {
                k + v.len() + digits + 1
            } else if after.starts_with("\":") {
                k + v.len() + digits + 2
            } else {
                from = k + 1;
                continue;
            };
            let start = colon + body[colon..].chars().take_while(|c| *c == '"' || *c == ' ').count();
            let end = body[start..].find(|c| c == '"' || c == ';' || c == '}').map(|x| start + x).unwrap_or(body.len());
            spans.push((start, end, theme));
            from = end.max(k + 1);
        }
    }
    spans.sort_by_key(|x| x.0);
    for (start, end, theme) in spans {
        if start >= last {
            out.push_str(&body[last..start]);
            out.push_str(&recolor(&body[start..end], theme, m));
            last = end;
        }
    }
    out.push_str(&body[last..]);
    out
}

/// CSS normal (también el que va dentro de los .js): `selector { declaraciones }`, con @media anidados.
fn theme_css(s: &str, m: &Mapper) -> (String, usize) {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut copied = 0; // hasta dónde se ha copiado `s` a `out`
    let mut sel_start = 0;
    let mut changed = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'}' => {
                sel_start = i + 1;
                i += 1;
            }
            b'{' => {
                let close = s[i + 1..].find('}').map(|x| i + 1 + x);
                let next_open = s[i + 1..].find('{').map(|x| i + 1 + x);
                match (close, next_open) {
                    (Some(c), n) if n.is_none_or(|n| n > c) => {
                        let sel = &s[sel_start..i];
                        if sel.len() < 4000 {
                            let body = &s[i + 1..c];
                            let nb = restyle(sel, body, m);
                            if nb != body {
                                out.push_str(&s[copied..i + 1]);
                                out.push_str(&nb);
                                copied = c;
                                changed += 1;
                            }
                        }
                        sel_start = c + 1;
                        i = c + 1;
                    }
                    _ => {
                        // bloque contenedor (@media …): las reglas de dentro empiezan aquí
                        sel_start = i + 1;
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    out.push_str(&s[copied..]);
    (out, changed)
}

/// Hoja ya analizada en JSON: `"selectorText":"…","style":{…}`.
fn theme_json(s: &str, m: &Mapper) -> (String, usize) {
    const SEL: &str = "\"selectorText\":\"";
    const STY: &str = "\"style\":{";
    let mut out = String::with_capacity(s.len());
    let mut copied = 0;
    let mut changed = 0;
    let mut pos = 0;
    while let Some(k) = s[pos..].find(SEL).map(|x| pos + x) {
        let a = k + SEL.len();
        let Some(e) = s[a..].find('"').map(|x| a + x) else { break };
        let sel = &s[a..e];
        pos = e;
        // el "style" de esta misma regla (antes del siguiente selector)
        let next_sel = s[e..].find(SEL).map(|x| e + x).unwrap_or(s.len());
        let Some(st) = s[e..next_sel].find(STY).map(|x| e + x + STY.len()) else { continue };
        let Some(end) = s[st..].find('}').map(|x| st + x) else { continue };
        let body = &s[st..end];
        let nb = restyle(sel, body, m);
        if nb != body {
            out.push_str(&s[copied..st]);
            out.push_str(&nb);
            copied = end;
            changed += 1;
        }
        pos = end;
    }
    out.push_str(&s[copied..]);
    (out, changed)
}

/// El texto sin sus colores (#hex, rgb(), rgba()): para saber si dos archivos solo se diferencian en eso.
fn strip_colors(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'#' && (i == 0 || !s[i - 1].is_ascii_alphanumeric()) {
            let n = s[i + 1..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            let end = i + 1 + n;
            if (3..=8).contains(&n) && (end >= s.len() || !s[end].is_ascii_alphanumeric()) {
                out.push(b'C');
                i = end;
                continue;
            }
        }
        if s[i..].starts_with(b"rgb(") || s[i..].starts_with(b"rgba(") {
            if let Some(c) = s[i..].iter().position(|&x| x == b')') {
                out.push(b'C');
                i += c + 1;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

/// ¿Son el mismo archivo salvo los colores? (un tema aplicado encima, no una actualización de Adobe)
pub fn same_but_colors(a: &[u8], b: &[u8]) -> bool {
    a == b || strip_colors(a) == strip_colors(b)
}

/// Aplica la paleta a un archivo de estilos de la pantalla de inicio. Devuelve None si no hay nada que cambiar.
/// `light`: paleta clara y grises de referencia del modo claro (None: el claro queda como el de Adobe).
pub fn theme_file(orig: &[u8], pal: &Pal, darkest: &ToneRef, dark: &ToneRef, light: Option<(&Pal, &ToneRef)>) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(orig).ok()?;
    let m = Mapper {
        darkest: (GreyMap::new(darkest, pal), darkest.accent),
        dark: (GreyMap::new(dark, pal), dark.accent),
        accent: pal.accent,
        light: light.map(|(lp, lt)| (GreyMap::new(lt, lp), lt.accent, lp.accent)),
    };
    let (out, n) = if s.trim_start().starts_with("{\"rules\"") { theme_json(s, &m) } else { theme_css(s, &m) };
    (n > 0).then(|| out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remap::Palette;

    fn mapper() -> (Pal, ToneRef) {
        (Palette::default().resolve().unwrap(), ToneRef::ae_darkest())
    }

    #[test]
    fn only_dark_rules_change() {
        let (pal, tone) = mapper();
        let css = ".spectrum--darkest .a{color:#1e1e1e;background:rgb(30, 30, 30)}.spectrum--light .a{color:#1e1e1e}@media (x){.spectrum--darkest{border-color:#fff}}";
        let out = String::from_utf8(theme_file(css.as_bytes(), &pal, &tone, &tone, None).unwrap()).unwrap();
        assert!(out.contains(".spectrum--light .a{color:#1e1e1e}"), "{out}");
        assert!(!out.contains(".spectrum--darkest .a{color:#1e1e1e"), "{out}");
        assert!(out.contains("@media (x){.spectrum--darkest{border-color:#"), "{out}");
    }

    #[test]
    fn json_rules() {
        let (pal, tone) = mapper();
        let j = r#"{"rules":[{"selectorText":".spectrum--darkest","style":{"background-color":"rgb(30, 30, 30)","x":"rgba(0,0,0,0)"}},{"selectorText":".spectrum--light","style":{"background-color":"rgb(30, 30, 30)"}}]}"#;
        let out = String::from_utf8(theme_file(j.as_bytes(), &pal, &tone, &tone, None).unwrap()).unwrap();
        assert!(out.contains(r#""selectorText":".spectrum--light","style":{"background-color":"rgb(30, 30, 30)"}"#), "{out}");
        assert!(out.contains("rgba(0,0,0,0)"), "{out}");
        assert!(!out.contains(r#"".spectrum--darkest","style":{"background-color":"rgb(30, 30, 30)""#), "{out}");
        serde_json::from_str::<serde_json::Value>(&out).unwrap();
    }
}

#[cfg(test)]
mod real {
    #[test]
    #[ignore]
    fn real_styles() {
        let p = r"C:\Program Files\Adobe\Adobe After Effects 2026\Support Files\UXP\plugins\com.adobe.ccx.start\css\styles.css";
        let b = std::fs::read(p).unwrap();
        let s = std::str::from_utf8(&b).unwrap();
        println!("json={}", s.trim_start().starts_with("{\"rules\""));
        let pal = crate::remap::Palette::default().resolve().unwrap();
        let t = crate::remap::ToneRef::ae_darkest();
        let m = super::Mapper { darkest: (crate::remap::GreyMap::new(&t, &pal), t.accent), dark: (crate::remap::GreyMap::new(&t, &pal), t.accent), accent: pal.accent, light: None };
        let (o, n) = super::theme_json(s, &m);
        println!("changed {n}, 30-30-30 left {}", o.matches("rgb(30, 30, 30)").count());
        let k = s.find("\"selectorText\":\".spectrum--darkest\"").unwrap();
        println!("{}", &s[k..k + 160]);
        println!("theme {:?}", super::theme_of(".spectrum--darkest"));
        for v in ["\"--color-dark-grey-3\"", "\"--spectrum-global-color-darkest-gray-100\"", "\"--spectrum-global-color-dark-gray-100\"", "\"--spectrum-global-color-light-gray-100\""] {
            let k = o.find(v).unwrap();
            println!("{}", &o[k..k + 70]);
        }
    }
}
