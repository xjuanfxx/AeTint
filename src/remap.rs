//! La paleta del usuario → cada color de una tabla de Adobe.
//!
//! Los grises de Adobe se reparten entre unos pocos "anclajes" (fondo, paneles, campos, marcos, texto).
//! Cada gris original cae entre dos anclajes por luminosidad y toma el color que queda en el mismo punto
//! entre los colores nuevos. Así los cientos de grises intermedios siguen la paleta sin listarlos uno a uno.

use crate::color::{self, Rgb};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Palette {
    /// Espacio entre paneles.
    pub base: String,
    /// Fondo de los paneles.
    pub panel: String,
    /// Filas de pestañas, barra de herramientas, columna derecha.
    pub frame: String,
    /// Campos, buscadores, fondo del visor.
    pub raised: String,
    /// Etiquetas y texto.
    pub text: String,
    /// Acento (selección, botones azules, foco).
    pub accent: String,
}

impl Default for Palette {
    fn default() -> Self {
        // Catppuccin Mocha
        Palette {
            base: "#11111B".into(),
            panel: "#1E1E2E".into(),
            frame: "#313244".into(),
            raised: "#181825".into(),
            text: "#CDD6F4".into(),
            accent: "#7A80C2".into(),
        }
    }
}

impl Palette {
    /// Paleta clara por defecto (Catppuccin Latte).
    pub fn light_default() -> Self {
        Palette {
            base: "#DCE0E8".into(),
            panel: "#EFF1F5".into(),
            frame: "#CCD0DA".into(),
            raised: "#E6E9EF".into(),
            text: "#4C4F69".into(),
            accent: "#7287FD".into(),
        }
    }
}

pub struct Pal {
    pub base: Rgb,
    pub panel: Rgb,
    pub frame: Rgb,
    pub raised: Rgb,
    pub text: Rgb,
    pub accent: Rgb,
}

impl Palette {
    pub fn resolve(&self) -> Result<Pal, String> {
        let p = |name: &str, h: &str| color::parse_hex(h).ok_or_else(|| tr!("invalid colour in “{}”: {}", "color no válido en «{}»: {}", name, h));
        Ok(Pal {
            base: p("fondo", &self.base)?,
            panel: p("paneles", &self.panel)?,
            frame: p("marcos", &self.frame)?,
            raised: p("campos", &self.raised)?,
            text: p("texto", &self.text)?,
            accent: p("acento", &self.accent)?,
        })
    }
}

/// Los grises de referencia de una tabla original (0..1).
#[derive(Debug, Clone, Copy)]
pub struct ToneRef {
    pub base: f32,
    pub panel: f32,
    pub raised: f32,
    pub label: f32,
    pub heading: Option<f32>,
    pub frame: Option<f32>,
    pub accent: Rgb,
}

impl ToneRef {
    /// Valores de la tabla "darkest" de After Effects 2026/2027, por si no hay un dvaui.dll que leer (vista previa).
    pub fn ae_darkest() -> Self {
        ToneRef {
            base: 0.0,
            panel: 29.0 / 255.0,
            raised: 14.0 / 255.0,
            label: 176.0 / 255.0,
            heading: Some(1.0),
            frame: Some(52.0 / 255.0),
            accent: [0.02353, 0.42353, 0.90588],
        }
    }

    /// Valores de la tabla "light" de After Effects 2026/2027 (vista previa del modo claro).
    pub fn ae_light() -> Self {
        ToneRef {
            base: 230.0 / 255.0,
            panel: 1.0,
            raised: 248.0 / 255.0,
            label: 80.0 / 255.0,
            heading: Some(0.0),
            frame: Some(230.0 / 255.0),
            accent: [2.0 / 255.0, 101.0 / 255.0, 220.0 / 255.0],
        }
    }

    /// Tabla clara: el texto es más oscuro que los paneles.
    pub fn is_light(&self) -> bool {
        self.label < self.panel
    }
}

pub struct GreyMap {
    pts: Vec<(f32, Rgb)>,
}

impl GreyMap {
    pub fn new(r: &ToneRef, p: &Pal) -> Self {
        let mut a: Vec<(f32, Rgb, u8)> = vec![
            // (gris, color, prioridad: menor gana si dos anclajes caen en la misma luminosidad)
            (r.base, p.base, 0),
            (r.panel, p.panel, 1),
            (r.raised, p.raised, 2),
            (r.label, p.text, 3),
        ];
        let light = r.is_light();
        // títulos: más contraste que el texto (más claros en oscuro, más oscuros en claro)
        if let Some(h) = r.heading {
            let toward = if light { [0.0; 3] } else { [1.0; 3] };
            a.push((h, color::mix(p.text, toward, if light { 0.5 } else { 0.7 }), 5));
        }
        // marcos: entre los paneles y el texto (en cualquiera de los dos sentidos)
        if let Some(f) = r.frame {
            let between = if light { f < r.panel && f > r.label } else { f > r.panel && f < r.label };
            if between && (f - r.base).abs() > 1e-3 {
                a.push((f, p.frame, 4));
            }
        }
        // extremos: negro y blanco
        if light {
            a.push((0.0, color::mix(p.text, [0.0; 3], 0.6), 9));
            a.push((1.0, color::mix(p.panel, [1.0; 3], 0.5), 9));
        } else {
            a.push((0.0, color::mix(p.base, [0.0; 3], 0.55), 9));
            a.push((1.0, color::mix(p.text, [1.0; 3], 0.8), 9));
        }
        let mut pts: Vec<(f32, Rgb, u8)> = a.into_iter().map(|(g, c, k)| (color::grey_l(g), c, k)).collect();
        pts.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap().then(x.2.cmp(&y.2)));
        let mut out: Vec<(f32, Rgb)> = Vec::new();
        for (l, c, _) in pts {
            if out.last().is_none_or(|last| l - last.0 > 1e-4) {
                out.push((l, c));
            }
        }
        GreyMap { pts: out }
    }

    pub fn map(&self, grey: f32) -> Rgb {
        let l = color::grey_l(grey);
        let p = &self.pts;
        if l <= p[0].0 {
            return p[0].1;
        }
        for k in 1..p.len() {
            if l <= p[k].0 {
                let t = (l - p[k - 1].0) / (p[k].0 - p[k - 1].0);
                return color::mix(p[k - 1].1, p[k].1, t);
            }
        }
        p[p.len() - 1].1
    }
}

/// Mueve un color de acento al tono y la fuerza del acento nuevo, conservando su paso de luminosidad
/// respecto al acento original (los estados hover/down siguen siendo más claros u oscuros).
pub fn shift_accent(c: Rgb, orig_accent: Rgb, new_accent: Rgb) -> Rgb {
    let k = color::to_oklch(c);
    let o = color::to_oklch(orig_accent);
    let n = color::to_oklch(new_accent);
    let l = (n[0] + (k[0] - o[0])).clamp(0.0, 1.0);
    let ch = if o[1] > 0.02 { n[1] * k[1] / o[1] } else { n[1] };
    color::from_oklch([l, ch.min(0.37), n[2]])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anchors_hit_exactly() {
        let pal = Palette::default().resolve().unwrap();
        let r = ToneRef::ae_darkest();
        let m = GreyMap::new(&r, &pal);
        assert_eq!(color::to_hex(m.map(r.panel)), "#1E1E2E");
        assert_eq!(color::to_hex(m.map(r.raised)), "#181825");
        assert_eq!(color::to_hex(m.map(r.label)), "#CDD6F4");
        assert_eq!(color::to_hex(m.map(r.base)), "#11111B");
    }
}
