//! Colores de dvaui.dll, el framework de interfaz de Adobe (paneles, botones, campos, listas).
//!
//! Dentro de .rdata hay:
//!   · una tabla de punteros a los nombres de los tokens de color, en orden alfabético
//!     ("accent-background-color-default" … "yellow-visual-color");
//!   · por cada brillo (light, dark, darkest…) una tabla de la misma longitud: un color por token, 4 float32 RGBA.
//! Además, dos recursos de texto: el JSON "DROVER-DNA-VARS" (barras de scroll, celdas) y el CSS "SPECTRUM_DARK"
//! (relleno de algunos iconos). Solo se reescriben números dentro de esas tablas y textos: el archivo conserva
//! su tamaño, sus secciones y su código.

use crate::color::{self, Rgb};
use crate::jsonfit;
use crate::pe::{self, f32le, u64le, Pe, ResKey, Resource};
use crate::remap::{shift_accent, GreyMap, Pal, ToneRef};
use serde_json::Value;

const NEED: &[&str] = &[
    "background-base-color",
    "background-layer-1-color",
    "background-layer-2-color",
    "gray-50",
    "gray-75",
    "gray-100",
    "gray-900",
    "accent-background-color-default",
    "body-color",
];
const RAMP: &[&str] = &["gray-50", "gray-75", "gray-100", "gray-200", "gray-300", "gray-400", "gray-500", "gray-600", "gray-700", "gray-800", "gray-900"];
pub const FRAME: &str = "drover-mix-menu-background-hover-down-or-focus";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Light,
    Dark,
    Darkest,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub off: usize,
    pub kind: Kind,
}

pub struct Dvaui {
    pub names: Vec<String>,
    pub tables: Vec<Table>,
    pub version: String,
    pub dna: Option<Resource>,
    pub css: Vec<Resource>,
    /// AE 2023/2024: los colores van en recursos JSON (texto "rgb(…)") en vez de tablas de float.
    pub legacy: Option<Legacy>,
}

/// Formato anterior a 2025: "DROVER" (colorThemes.light/dark/darkest), "DNA-VARS-REDUCED" (light/dark/darkest)
/// y "VARIABLES" (tokens con sets por brillo).
/// AE 2020/2021 no tienen "DROVER": sus colores van en "DNA-VARS" y "DNA-VARS-LINKED" (light/dark/darkest con
/// tokens `spectrum-…`); entonces `drover` es `None`, `reduced` es DNA-VARS y `linked` DNA-VARS-LINKED.
pub struct Legacy {
    pub drover: Option<Resource>,
    pub reduced: Option<Resource>,
    pub linked: Option<Resource>,
    pub variables: Option<Resource>,
    pub tones: Vec<(Kind, ToneRef)>,
}

const BLOCKS: [(&str, Kind); 3] = [("darkest", Kind::Darkest), ("dark", Kind::Dark), ("light", Kind::Light)];

fn res_json(b: &[u8], r: &Resource) -> Option<Value> {
    let s = String::from_utf8_lossy(&b[r.off..r.off + r.size]);
    serde_json::from_str(s.trim_start_matches('\u{feff}').trim_end_matches(['\0', ' ', '\n', '\r', '\t'])).ok()
}

/// Los grises de referencia de un bloque de colorThemes (formato 2023/2024).
fn legacy_tone(m: &serde_json::Map<String, Value>, kind: Kind) -> Option<ToneRef> {
    let c = |n: &str| m.get(n).and_then(Value::as_str).and_then(color::parse_css).map(|x| x.0);
    let g = |n: &str| c(n).filter(|x| color::is_grey(*x)).map(|x| x[0]);
    // en los oscuros, los "background-*" de 2023/2024 no son los fondos que usa AE; la escala de grises sí
    // coincide con las tablas de 2025 (gray-50 fondo, gray-75 elevado, gray-100 paneles)
    let (base, raised, panel) = if kind == Kind::Light {
        (g("background-base-color")?, g("background-layer-1-color")?, g("background-layer-2-color")?)
    } else {
        (g("gray-50")?, g("gray-75")?, g("gray-100")?)
    };
    Some(ToneRef {
        base,
        panel,
        raised,
        label: if kind == Kind::Light { 80.0 / 255.0 } else { 176.0 / 255.0 },
        heading: g("heading-color"),
        frame: g(FRAME),
        accent: c("accent-background-color-default")?,
    })
}

/// Los grises de referencia de un bloque de DNA-VARS (AE 2020/2021): la escala `spectrum-global-color-gray-*`
/// (fondo, elevado y paneles) y el color de foco como acento.
fn dna_tone(m: &serde_json::Map<String, Value>, kind: Kind) -> Option<ToneRef> {
    let c = |n: &str| m.get(n).and_then(Value::as_str).and_then(color::parse_css).map(|x| x.0);
    let g = |n: &str| c(&format!("spectrum-global-color-{n}")).filter(|x| color::is_grey(*x)).map(|x| x[0]);
    let (base, raised, panel) = if kind == Kind::Light {
        (g("gray-200")?, g("gray-75")?, g("gray-50")?)
    } else {
        (g("gray-50")?, g("gray-75")?, g("gray-100")?)
    };
    Some(ToneRef {
        base,
        panel,
        raised,
        label: if kind == Kind::Light { 80.0 / 255.0 } else { 176.0 / 255.0 },
        heading: c("spectrum-alias-heading-text-color").filter(|x| color::is_grey(*x)).map(|x| x[0]),
        frame: None,
        accent: c("spectrum-alias-focus-color")?,
    })
}

/// ¿Un color del tono del acento de Adobe? (DNA-VARS no marca los colores de acento por nombre: los azules
/// saturados con el mismo tono que el acento de referencia se llevan al acento nuevo)
fn near_accent(c: Rgb, accent: Rgb) -> bool {
    let (k, a) = (color::to_oklch(c), color::to_oklch(accent));
    let mut d = (k[2] - a[2]).abs();
    if d > std::f32::consts::PI {
        d = 2.0 * std::f32::consts::PI - d;
    }
    k[1] > 0.08 && d < 0.35
}

fn cstr(b: &[u8], o: usize) -> Option<&str> {
    let mut e = o;
    while e < b.len() && e - o < 96 && b[e] != 0 {
        if !(0x20..=0x7e).contains(&b[e]) {
            return None;
        }
        e += 1;
    }
    if e == o || e >= b.len() || b[e] != 0 {
        return None;
    }
    std::str::from_utf8(&b[o..e]).ok()
}

fn find(hay: &[u8], needle: &[u8], from: usize, to: usize) -> Option<usize> {
    hay[from..to].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

impl Dvaui {
    pub fn open(b: &[u8]) -> Result<Dvaui, String> {
        let p = Pe::parse(b)?;
        let res = p.resources(b);
        let rd = p.sec(".rdata").ok_or_else(|| tr!("dvaui.dll has no .rdata section", "dvaui.dll sin sección .rdata"))?;
        let (r0, r1) = (rd.raw as usize, (rd.raw + rd.raw_size) as usize);

        // 1. la tabla de nombres
        let key = b"\0background-layer-1-color\0";
        let Some(at) = find(b, key, r0, r1) else {
            return Self::open_legacy(b, &res);
        };
        let at = at + 1;
        let va = p.image_base + p.rva(at).ok_or_else(|| tr!("invalid RVA", "RVA no válida"))? as u64;
        let ptr = va.to_le_bytes();
        let mut q = None;
        let mut i = r0;
        while i + 8 <= r1 {
            if b[i..i + 8] == ptr {
                q = Some(i);
                break;
            }
            i += 8;
        }
        let q = q.ok_or_else(|| tr!("colour token name table not found", "no se encontró la tabla de nombres"))?;
        let str_at = |q: usize| -> Option<&str> {
            if q < r0 || q + 8 > r1 {
                return None;
            }
            let v = u64le(b, q).checked_sub(p.image_base)?;
            if v > 0x7fff_ffff {
                return None;
            }
            cstr(b, p.off(v as u32)?)
        };
        let (mut s, mut e) = (q, q);
        while let (Some(prev), Some(cur)) = (str_at(s - 8), str_at(s)) {
            if prev < cur { s -= 8 } else { break }
        }
        while let (Some(next), Some(cur)) = (str_at(e + 8), str_at(e)) {
            if next > cur { e += 8 } else { break }
        }
        let names: Vec<String> = (s..=e).step_by(8).filter_map(|q| str_at(q).map(String::from)).collect();
        for n in NEED {
            if !names.iter().any(|x| x == n) {
                return Err(tr!("missing token {}", "falta el token {}", n));
            }
        }

        // 2. las tablas de color
        let n = names.len();
        let id = |s: &str| names.iter().position(|x| x == s);
        let ramp: Vec<usize> = RAMP.iter().filter_map(|r| id(r)).collect();
        let (ib, il1) = (id("background-base-color").unwrap(), id("background-layer-1-color").unwrap());
        let mut tables = Vec::new();
        let mut o = r0;
        while o + n * 16 <= r1 {
            if let Some(kind) = Self::probe(b, o, n, &ramp, ib, il1) {
                tables.push(Table { off: o, kind });
                o += n * 16;
            } else {
                o += 16;
            }
        }
        if !tables.iter().any(|t| t.kind != Kind::Light) {
            return Err(tr!("no dark tables with the original greys (is a theme already applied?)", "no hay tablas oscuras con los grises originales (¿el archivo ya tiene un tema aplicado?)"));
        }

        let dna = res.iter().find(|r| r.is("JSON", "DROVER-DNA-VARS")).cloned();
        let css = res
            .iter()
            .filter(|r| matches!(r.path.first(), Some(ResKey::Name(t)) if t == "CSS") && matches!(r.path.get(1), Some(ResKey::Name(n)) if n == "SPECTRUM_DARK"))
            .cloned()
            .collect();
        Ok(Dvaui { names, tables, version: pe::file_version(b, &res), dna, css, legacy: None })
    }

    fn open_legacy(b: &[u8], res: &[Resource]) -> Result<Dvaui, String> {
        let get = |n: &str| res.iter().find(|r| r.is("JSON", n)).cloned();
        let Some(drover) = get("DROVER") else {
            return Self::open_dna(b, res);
        };
        let j = res_json(b, &drover).ok_or_else(|| tr!("DROVER: unreadable JSON", "DROVER: JSON ilegible"))?;
        let mut tones = Vec::new();
        for (name, kind) in BLOCKS {
            if let Some(Value::Object(m)) = j["colorThemes"].get(name) {
                if let Some(t) = legacy_tone(m, kind) {
                    tones.push((kind, t));
                }
            }
        }
        if !tones.iter().any(|(k, _)| *k != Kind::Light) {
            return Err(tr!("no dark tables with the original greys (is a theme already applied?)", "no hay tablas oscuras con los grises originales (¿el archivo ya tiene un tema aplicado?)"));
        }
        let css = res
            .iter()
            .filter(|r| matches!(r.path.first(), Some(ResKey::Name(t)) if t == "CSS") && matches!(r.path.get(1), Some(ResKey::Name(n)) if n == "SPECTRUM_DARK"))
            .cloned()
            .collect();
        Ok(Dvaui {
            names: Vec::new(),
            tables: Vec::new(),
            version: pe::file_version(b, res),
            dna: get("DROVER-DNA-VARS"),
            css,
            legacy: Some(Legacy { drover: Some(drover), reduced: get("DNA-VARS-REDUCED"), linked: None, variables: get("VARIABLES"), tones }),
        })
    }

    /// AE 2020/2021: sin "DROVER"; los colores están en "DNA-VARS" (y sus referencias en "DNA-VARS-LINKED").
    fn open_dna(b: &[u8], res: &[Resource]) -> Result<Dvaui, String> {
        let get = |n: &str| res.iter().find(|r| r.is("JSON", n)).cloned();
        let vars = get("DNA-VARS").ok_or(tr!("no colour tokens found in dvaui.dll (unsupported version)", "no se encontraron los tokens de color en dvaui.dll (versión no compatible)"))?;
        let j = res_json(b, &vars).ok_or_else(|| tr!("DNA-VARS: unreadable JSON", "DNA-VARS: JSON ilegible"))?;
        let mut tones = Vec::new();
        for (name, kind) in BLOCKS {
            if let Some(Value::Object(m)) = j.get(name) {
                if let Some(t) = dna_tone(m, kind) {
                    tones.push((kind, t));
                }
            }
        }
        if !tones.iter().any(|(k, _)| *k != Kind::Light) {
            return Err(tr!("no dark tables with the original greys (is a theme already applied?)", "no hay tablas oscuras con los grises originales (¿el archivo ya tiene un tema aplicado?)"));
        }
        let css = res
            .iter()
            .filter(|r| matches!(r.path.first(), Some(ResKey::Name(t)) if t == "CSS") && matches!(r.path.get(1), Some(ResKey::Name(n)) if n == "SPECTRUM_DARK"))
            .cloned()
            .collect();
        // los colores propios de dvaui (barras de scroll, celdas): "DROVER-DNA-VARS" en 2021, "DROVER-VARS" en 2020
        let dna = get("DROVER-DNA-VARS").or_else(|| get("DROVER-VARS"));
        Ok(Dvaui {
            names: Vec::new(),
            tables: Vec::new(),
            version: pe::file_version(b, res),
            dna,
            css,
            legacy: Some(Legacy { drover: None, reduced: Some(vars), linked: get("DNA-VARS-LINKED"), variables: None, tones }),
        })
    }

    fn probe(b: &[u8], o: usize, n: usize, ramp: &[usize], ib: usize, il1: usize) -> Option<Kind> {
        let grey = |i: usize| -> Option<f32> {
            let q = o + i * 16;
            let (r, g, bl, a) = (f32le(b, q), f32le(b, q + 4), f32le(b, q + 8), f32le(b, q + 12));
            (r == g && g == bl && a == 1.0 && (0.0..=1.0).contains(&r)).then_some(r)
        };
        grey(ramp[0])?;
        let v: Option<Vec<f32>> = ramp.iter().map(|&i| grey(i)).collect();
        let v = v?;
        let up = v.windows(2).all(|w| w[1] > w[0]);
        let down = v.windows(2).all(|w| w[1] < w[0]);
        if !up && !down {
            return None;
        }
        if !(0..n * 4).all(|k| (0.0..=1.0).contains(&f32le(b, o + k * 4))) {
            return None;
        }
        if f32le(b, o + ib * 16) != v[0] && (f32le(b, o + il1 * 16) - v[1]).abs() > 0.2 {
            return None;
        }
        Some(if down { Kind::Light } else if v[0] < 0.06 { Kind::Darkest } else { Kind::Dark })
    }

    fn id(&self, n: &str) -> Option<usize> {
        self.names.iter().position(|x| x == n)
    }

    fn read(&self, b: &[u8], t: &Table, i: usize) -> [f32; 4] {
        let q = t.off + i * 16;
        [f32le(b, q), f32le(b, q + 4), f32le(b, q + 8), f32le(b, q + 12)]
    }

    /// Los grises de referencia de una tabla original.
    pub fn tone(&self, b: &[u8], t: &Table) -> ToneRef {
        let g = |n: &str| self.id(n).map(|i| self.read(b, t, i)[0]);
        let acc = self.read(b, t, self.id("accent-background-color-default").unwrap());
        ToneRef {
            base: g("background-base-color").unwrap(),
            panel: g("background-layer-2-color").unwrap(),
            raised: g("background-layer-1-color").unwrap(),
            // en las tablas claras las etiquetas son grises oscuros (el texto de la paleta clara)
            label: if t.kind == Kind::Light { 80.0 / 255.0 } else { 176.0 / 255.0 },
            heading: g("heading-color"),
            frame: g(FRAME),
            accent: [acc[0], acc[1], acc[2]],
        }
    }

    /// El tono de referencia para un brillo (el primero de ese tipo).
    pub fn tone_for(&self, b: &[u8], kind: Kind) -> Option<ToneRef> {
        if let Some(l) = &self.legacy {
            return l.tones.iter().find(|t| t.0 == kind).map(|t| t.1);
        }
        self.tables.iter().find(|t| t.kind == kind).map(|t| self.tone(b, t))
    }

    /// Aplica la paleta sobre una copia del archivo original: `pal` en Darkest y Dark, `light` en Light
    /// (si no hay paleta clara, el modo claro de AE queda como el de Adobe).
    pub fn apply(&self, orig: &[u8], pal: &Pal, light: Option<&Pal>) -> Result<Vec<u8>, String> {
        let mut b = orig.to_vec();
        for t in &self.tables {
            let pal = match (t.kind, light) {
                (Kind::Light, Some(l)) => l,
                (Kind::Light, None) => continue,
                _ => pal,
            };
            let tone = self.tone(orig, t);
            let map = GreyMap::new(&tone, pal);
            for (i, name) in self.names.iter().enumerate() {
                let [r, g, bl, a] = self.read(orig, t, i);
                let c: Rgb = [r, g, bl];
                let new = if is_accent(name) && !color::is_grey(c) {
                    shift_accent(c, tone.accent, pal.accent)
                } else if color::is_grey(c) && !keep_grey(name) {
                    map.map((r + g + bl) / 3.0)
                } else {
                    continue;
                };
                let q = t.off + i * 16;
                for (k, v) in [new[0], new[1], new[2], a].into_iter().enumerate() {
                    b[q + k * 4..q + k * 4 + 4].copy_from_slice(&v.clamp(0.0, 1.0).to_le_bytes());
                }
            }
        }

        // AE 2023/2024: los mismos cambios sobre los recursos JSON
        if let Some(l) = &self.legacy {
            self.apply_legacy(l, orig, &mut b, pal, light)?;
        }

        // DROVER-DNA-VARS: los grises opacos de los bloques oscuros
        if let Some(r) = &self.dna {
            let mut tones: Vec<(String, ToneRef, &Pal)> = [("darkest", Kind::Darkest), ("dark", Kind::Dark)]
                .into_iter()
                .filter_map(|(k, kind)| self.tone_for(orig, kind).map(|t| (k.to_string(), t, pal)))
                .collect();
            if let (Some(l), Some(t)) = (light, self.tone_for(orig, Kind::Light)) {
                tones.push(("light".into(), t, l));
            }
            let text = &orig[r.off..r.off + r.size];
            let new = jsonfit::rewrite_same_len(text, |v| {
                for (block, tone, pal) in &tones {
                    let map = GreyMap::new(tone, pal);
                    if let Some(Value::Object(m)) = v.get_mut(block) {
                        for val in m.values_mut() {
                            if let Value::String(s) = val {
                                if let Some(n) = remap_css_grey(s, &map) {
                                    *s = n;
                                }
                            }
                        }
                    }
                }
                Ok(())
            })?;
            b[r.off..r.off + r.size].copy_from_slice(&new);
        }

        // SPECTRUM_DARK: ".blue { fill: #XXXXXX }" → acento (mismo número de caracteres)
        let hex = color::to_hex(pal.accent);
        for r in &self.css {
            let s = &mut b[r.off..r.off + r.size];
            if let Some(p) = find(s, b".blue", 0, s.len()) {
                if let Some(f) = find(s, b"fill:", p, s.len().min(p + 80)) {
                    if let Some(h) = find(s, b"#", f, s.len().min(f + 12)) {
                        if h + 7 <= s.len() && s[h + 1..h + 7].iter().all(u8::is_ascii_hexdigit) {
                            s[h + 1..h + 7].copy_from_slice(&hex.as_bytes()[1..7]);
                        }
                    }
                }
            }
        }
        pe::fix_checksum(&mut b);
        Ok(b)
    }
}

impl Dvaui {
    /// AE 2023/2024: aplica la paleta a "DROVER", "DNA-VARS-REDUCED" y "VARIABLES" (mismas reglas que las tablas).
    fn apply_legacy(&self, l: &Legacy, orig: &[u8], b: &mut [u8], pal: &Pal, light: Option<&Pal>) -> Result<(), String> {
        let maps: Vec<(&str, GreyMap, ToneRef, &Pal)> = BLOCKS
            .iter()
            .filter_map(|(name, kind)| {
                let p = match (kind, light) {
                    (Kind::Light, Some(lp)) => lp,
                    (Kind::Light, None) => return None,
                    _ => pal,
                };
                let t = l.tones.iter().find(|t| t.0 == *kind)?.1;
                Some((*name, GreyMap::new(&t, p), t, p))
            })
            .collect();
        // AE 2020/2021 (sin DROVER): los acentos se reconocen por su tono, no por el nombre
        let dna_only = l.drover.is_none();
        let recolor = |name: &str, s: &mut String, block: &str| {
            let Some((_, map, tone, p)) = maps.iter().find(|m| m.0 == block) else { return };
            let Some((c, alpha)) = color::parse_css(s) else { return };
            // los colores con alfa (< 1) son veladuras: se dejan
            if alpha.as_deref().is_some_and(|a| a.parse::<f32>().map_or(true, |x| x < 1.0)) {
                return;
            }
            let accent = !color::is_grey(c) && (is_accent(name) || (dna_only && near_accent(c, tone.accent)));
            let new = if accent {
                shift_accent(c, tone.accent, p.accent)
            } else if color::is_grey(c) && !keep_grey(name) {
                map.map((c[0] + c[1] + c[2]) / 3.0)
            } else {
                return;
            };
            *s = color::css(new, alpha.as_deref());
        };
        let mut patch = |r: &Resource, f: &mut dyn FnMut(&mut Value)| -> Result<(), String> {
            let new = jsonfit::rewrite_same_len(&orig[r.off..r.off + r.size], |v| {
                f(v);
                Ok(())
            })?;
            b[r.off..r.off + r.size].copy_from_slice(&new);
            Ok(())
        };
        let flat = |v: &mut Value, prefix: &str| {
            for (block, ..) in &maps {
                if let Some(Value::Object(m)) = v.get_mut(*block) {
                    for (k, val) in m.iter_mut() {
                        if let Value::String(s) = val {
                            recolor(k.strip_prefix(prefix).unwrap_or(k), s, block);
                        }
                    }
                }
            }
        };
        // DROVER: colorThemes.<brillo>.<token>
        if let Some(r) = &l.drover {
            patch(r, &mut |v| {
                if let Some(t) = v.get_mut("colorThemes") {
                    flat(t, "");
                }
            })?;
        }
        // DNA-VARS-REDUCED (2022–2024) o DNA-VARS y DNA-VARS-LINKED (2020/2021): <brillo>.spectrum-<componente>-…
        // (en LINKED casi todo son referencias "$spectrum-…", que no se tocan; solo los colores literales)
        for r in [&l.reduced, &l.linked].into_iter().flatten() {
            patch(r, &mut |v| flat(v, "spectrum-"))?;
        }
        // VARIABLES: <token>.sets.<brillo>.value (puede anidarse)
        fn walk(token: &str, v: &mut Value, f: &dyn Fn(&str, &mut String, &str)) {
            let Some(Value::Object(sets)) = v.get_mut("sets") else { return };
            for (block, inner) in sets.iter_mut() {
                if let Some(Value::String(s)) = inner.get_mut("value") {
                    f(token, s, block);
                }
                walk(token, inner, f);
            }
        }
        if let Some(r) = &l.variables {
            patch(r, &mut |v| {
                if let Value::Object(m) = v {
                    for (k, val) in m.iter_mut() {
                        walk(k, val, &recolor);
                    }
                }
            })?;
        }
        Ok(())
    }
}

pub fn is_accent(n: &str) -> bool {
    const P: &[&str] = &[
        "accent-",
        "informative-",
        "focus-indicator",
        "drover-mix-accent",
        "blue-",
        "drover-mix-table-selected",
        "card-selection",
        "drop-zone-background",
        "drover-mix-drop-zone",
        "navigational-indicator",
        "static-blue",
    ];
    P.iter().any(|p| n.starts_with(p))
}

pub fn keep_grey(n: &str) -> bool {
    n == "black" || n == "white" || n.starts_with("static-") || n.starts_with("transparent") || n.starts_with("drop-shadow") || n.ends_with("-shadow") || n.contains("-shadow-")
}

/// Un "rgb(…)" gris opaco → su color en la paleta. Los colores con alfa (< 1) son veladuras: se dejan.
pub fn remap_css_grey(s: &str, map: &GreyMap) -> Option<String> {
    let (c, alpha) = color::parse_css(s)?;
    if !color::is_grey(c) || alpha.as_deref().is_some_and(|a| a.parse::<f32>().map_or(true, |x| x < 1.0)) {
        return None;
    }
    Some(color::css(map.map((c[0] + c[1] + c[2]) / 3.0), alpha.as_deref()))
}

#[cfg(test)]
mod real {
    #[test]
    #[ignore]
    fn light_tone() {
        let p = format!(r"{}\AeTint\originals\adobe-after-effects-2026\dvaui.dll", std::env::var("LOCALAPPDATA").unwrap());
        let b = std::fs::read(p).unwrap();
        let d = super::Dvaui::open(&b).unwrap();
        for t in &d.tables {
            let tone = d.tone(&b, t);
            let g = |n: &str| d.id(n).map(|i| (d.read(&b, t, i)[0] * 255.0).round());
            println!("{:?} base {:.0} panel {:.0} raised {:.0} frame {:?} heading {:?} body {:?} accent {:?}", t.kind, tone.base * 255.0, tone.panel * 255.0, tone.raised * 255.0, g(super::FRAME), g("heading-color"), g("body-color"), tone.accent.map(|x| (x * 255.0).round()));
        }
    }
}
