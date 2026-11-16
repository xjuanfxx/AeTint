//! Colores propios de After Effects, en AfterFXLib.dll: el recurso JSON "AE-SPECTRUM-COLORTHEMES".
//!
//! Son ~120 colores que dvaui.dll no controla: la regla de tiempo y el navegador de la timeline, el área de trabajo,
//! el indicador de tiempo (CTI), las barras de capa, el graph editor, marcadores, render queue, tooltips…
//! Bloques "light", "dark" y "darkest"; las claves son "ae-color-…" y los valores "rgb(…)" / "rgba(…)".

use crate::color;
use crate::dvaui::remap_css_grey;
use crate::jsonfit;
use crate::pe::{self, Pe, Resource};
use crate::remap::{GreyMap, Pal, ToneRef};
use serde_json::Value;
use std::collections::BTreeMap;

pub const RES_NAME: &str = "AE-SPECTRUM-COLORTHEMES";
pub const PREFIX: &str = "ae-color-";

pub struct AeLib {
    res: Resource,
    pub version: String,
}

/// Grupos de claves que la interfaz deja cambiar a mano (además de seguir la paleta).
pub const GROUPS: &[(&str, &str, &[&str])] = &[
    ("navigator", "Navigator", &["time-ruler-abs-grad-start", "time-ruler-abs-grad-end"]),
    ("workarea", "Work area", &["time-ruler-work-area-grad-start", "time-ruler-work-area-grad-end"]),
    ("cti", "Playhead (CTI)", &["time-ruler-cti-comp", "time-ruler-cti-layer"]),
    ("layersel", "Selected layer", &["tl-layer-bar-sel-grad-start", "tl-layer-bar-sel-grad-end"]),
    ("keyframe", "Keyframe", &["timelinegekeyframe-selected"]),
];

impl AeLib {
    pub fn open(b: &[u8]) -> Result<AeLib, String> {
        let p = Pe::parse(b)?;
        let res = p.resources(b);
        let r = res.iter().find(|r| r.is("JSON", RES_NAME)).cloned().ok_or("AfterFXLib.dll no tiene la tabla de colores de AE")?;
        Ok(AeLib { res: r, version: pe::file_version(b, &res) })
    }

    pub fn json(&self, b: &[u8]) -> Result<Value, String> {
        let t = &b[self.res.off..self.res.off + self.res.size];
        let s = String::from_utf8_lossy(t);
        let s = s.trim_start_matches('\u{feff}').trim_end_matches(['\0', ' ', '\n', '\r', '\t']);
        serde_json::from_str(s).map_err(|e| format!("JSON de AE ilegible: {e}"))
    }

    /// ¿El bloque darkest todavía tiene los grises de Adobe? (fondo de la timeline gris neutro)
    pub fn looks_original(&self, b: &[u8]) -> bool {
        self.json(b)
            .ok()
            .and_then(|v| v["darkest"]["ae-color-timelinegebackground"].as_str().and_then(color::parse_css))
            .is_some_and(|(c, _)| color::is_grey(c))
    }

    /// Aplica la paleta (grises → paleta) y los colores a mano (`overrides`, claves sin "ae-color-").
    /// `blocks`: (bloque "darkest"/"dark"/"light", grises de referencia, paleta). Los colores a mano solo van
    /// en los bloques oscuros: en el claro no tendrían sentido.
    pub fn apply(&self, orig: &[u8], blocks: &[(&str, ToneRef, &Pal)], overrides: &BTreeMap<String, String>) -> Result<Vec<u8>, String> {
        let mut b = orig.to_vec();
        let r = &self.res;
        let none = BTreeMap::new();
        let new = jsonfit::rewrite_same_len(&orig[r.off..r.off + r.size], |v| {
            for (block, tone, pal) in blocks {
                let map = GreyMap::new(tone, pal);
                let overrides = if *block == "light" { &none } else { overrides };
                let Some(Value::Object(m)) = v.get_mut(*block) else { continue };
                for (k, val) in m.iter_mut() {
                    let Value::String(s) = val else { continue };
                    let short = k.strip_prefix(PREFIX).unwrap_or(k);
                    if let Some(hex) = overrides.get(short) {
                        let c = color::parse_hex(hex).ok_or_else(|| format!("color no válido para {short}: {hex}"))?;
                        let alpha = color::parse_css(s).and_then(|(_, a)| a);
                        *s = color::css(c, alpha.as_deref());
                    } else if let Some(n) = remap_css_grey(s, &map) {
                        *s = n;
                    }
                }
            }
            Ok(())
        })?;
        b[r.off..r.off + r.size].copy_from_slice(&new);
        pe::fix_checksum(&mut b);
        Ok(b)
    }
}

/// Los colores que tendría cada clave en un bloque ("darkest" o "light") para la vista previa, sin tocar archivos.
pub fn preview(defaults: &Value, block: &str, pal: &Pal, tone: &ToneRef, overrides: &BTreeMap<String, String>) -> BTreeMap<String, color::Rgb> {
    let map = GreyMap::new(tone, pal);
    let none = BTreeMap::new();
    let overrides = if block == "light" { &none } else { overrides };
    let mut out = BTreeMap::new();
    if let Some(Value::Object(m)) = defaults.get(block) {
        for (k, v) in m {
            let short = k.strip_prefix(PREFIX).unwrap_or(k).to_string();
            let Some(s) = v.as_str() else { continue };
            let Some((c, _)) = color::parse_css(s) else { continue };
            let c = if let Some(h) = overrides.get(&short).and_then(|h| color::parse_hex(h)) {
                h
            } else if let Some(n) = remap_css_grey(s, &map) {
                color::parse_css(&n).map(|x| x.0).unwrap_or(c)
            } else {
                c
            };
            out.insert(short, c);
        }
    }
    out
}
