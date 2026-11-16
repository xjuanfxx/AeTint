//! Pantalla de inicio (splash) propia. Cada splash de AE es un PNG entero: a la izquierda un panel liso con el
//! logo ("Adobe" / "After Effects"), a la derecha la ilustración fundida con el panel, y las esquinas redondeadas
//! transparentes. AeTint pone la imagen del usuario solo en la parte de la ilustración (con el mismo fundido),
//! pinta el panel y el logo con la paleta y conserva la transparencia de las esquinas.
//!
//! Se escribe en los tres PNG de AfterFXLib.dll ("AE_SPLASH", "AE_SPLASH_AT_3TO2X" y "AE_SPLASH_AT_2X") en su
//! sitio, sin cambiar el tamaño del archivo: se comprime y el hueco que sobra se rellena con un bloque PNG que
//! los lectores ignoran (si no cabe a todo color, a 256 colores). "Restaurar" lo deshace con el resto del archivo.

use crate::pe::Resource;
use crate::remap::Pal;
use image::{imageops::FilterType, DynamicImage, Rgba, RgbaImage};

pub const NAMES: &[&str] = &["AE_SPLASH", "AE_SPLASH_AT_3TO2X", "AE_SPLASH_AT_2X"];

/// Ancho y alto de un PNG (cabecera IHDR).
fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    Some((u32::from_be_bytes(b[16..20].try_into().ok()?), u32::from_be_bytes(b[20..24].try_into().ok()?)))
}

/// Recorta (centrado) y escala la imagen para cubrir w × h.
fn cover(img: &DynamicImage, w: u32, h: u32) -> RgbaImage {
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let s = (w as f64 / iw).max(h as f64 / ih);
    let (cw, ch) = ((w as f64 / s).round().max(1.0) as u32, (h as f64 / s).round().max(1.0) as u32);
    let (x, y) = (((iw as u32).saturating_sub(cw)) / 2, ((ih as u32).saturating_sub(ch)) / 2);
    img.crop_imm(x, y, cw.min(iw as u32), ch.min(ih as u32)).resize_exact(w, h, FilterType::Lanczos3).to_rgba8()
}

fn diff(a: &Rgba<u8>, b: &Rgba<u8>) -> u32 {
    (0..3).map(|i| (a[i] as i32 - b[i] as i32).unsigned_abs()).sum()
}

/// Rectángulo de la ilustración (x0, y0, x1, y1, inclusive): lo que deja de ser el panel liso, buscando desde
/// un cuarto del ancho. En 2025+ llega a los bordes; en 2024 es un recuadro redondeado con márgenes.
fn art_box(orig: &RgbaImage, panel: &Rgba<u8>) -> (u32, u32, u32, u32) {
    let (w, h) = orig.dimensions();
    // una columna (o fila) es ilustración si más de un tercio deja de ser el panel: el texto del logo ocupa poco
    let col = |x: u32| {
        let ys: Vec<u32> = (0..h).step_by(3).collect();
        ys.iter().filter(|&&y| diff(orig.get_pixel(x, y), panel) > 24).count() as f32 / ys.len() as f32
    };
    let x0 = ((w / 4)..w).find(|&x| col(x) > 0.34).unwrap_or((w as f32 * 0.43) as u32);
    let x1 = (x0..w).rev().find(|&x| col(x) > 0.34).unwrap_or(w - 1);
    let row = |y: u32| {
        let xs: Vec<u32> = (x0..=x1).step_by(3).collect();
        xs.iter().filter(|&&x| diff(orig.get_pixel(x, y), panel) > 24).count() as f32 / xs.len() as f32
    };
    let y0 = (0..h).find(|&y| row(y) > 0.34).unwrap_or(0);
    let y1 = (y0..h).rev().find(|&y| row(y) > 0.34).unwrap_or(h - 1);
    (x0, y0, x1, y1)
}

/// Cobertura (0..1) de un rectángulo redondeado en (x, y), con borde suavizado.
fn rounded(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    (r + 0.5 - d).clamp(0.0, 1.0)
}

fn to8(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| (v * 255.0).clamp(0.0, 255.0))
}

/// El panel y el logo con la paleta: el fondo con el color de los paneles, lo saturado ("Adobe") con el acento y
/// lo neutro ("After Effects") con el color de texto, conservando el suavizado de las letras.
fn recolor_panel(c: &Rgba<u8>, panel: &Rgba<u8>, pal: &Pal) -> [f32; 3] {
    let reach = (0..3).map(|i| (panel[i] as f32).max(255.0 - panel[i] as f32)).fold(1.0f32, f32::max);
    let a = ((0..3).map(|i| (c[i] as f32 - panel[i] as f32).abs()).fold(0.0f32, f32::max) / reach).clamp(0.0, 1.0);
    let (mx, mn) = (c[0].max(c[1]).max(c[2]) as f32, c[0].min(c[1]).min(c[2]) as f32);
    let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
    let fg = to8(if sat > 0.35 { pal.accent } else { pal.text });
    let bg = to8(pal.panel);
    [0, 1, 2].map(|i| bg[i] + (fg[i] - bg[i]) * a)
}

/// La imagen del usuario en la zona de la ilustración, el panel con la paleta y las esquinas como estaban.
fn compose(orig: &RgbaImage, user: &DynamicImage, pal: Option<&Pal>) -> RgbaImage {
    let (w, h) = orig.dimensions();
    let panel = *orig.get_pixel((w as f32 * 0.03) as u32, (h as f32 * 0.6) as u32);
    let (x0, y0, x1, y1) = art_box(orig, &panel);
    // ¿la ilustración llega a los bordes (fundido por la izquierda) o es un recuadro con márgenes?
    let edge = x1 + 3 >= w && y0 <= 3 && y1 + 3 >= h;
    let fade = ((w as f32 * 0.07) as u32).max(1);
    let (ax, ay) = if edge { (x0.saturating_sub(fade / 3), 0) } else { (x0, y0) };
    let (aw, ah) = if edge { (w - ax, h) } else { (x1 - x0 + 1, y1 - y0 + 1) };
    let art = cover(user, aw, ah);
    let radius = w as f32 * 0.027;
    let flat = match pal {
        Some(p) => to8(p.panel),
        None => [panel[0] as f32, panel[1] as f32, panel[2] as f32],
    };
    let mut out = orig.clone();
    for y in 0..h {
        for x in 0..w {
            let o = orig.get_pixel(x, y);
            let in_box = x >= ax && x < ax + aw && y >= ay && y < ay + ah;
            // fuera de la ilustración: el panel (y el logo) con la paleta
            let left = if in_box {
                flat
            } else {
                match pal {
                    Some(p) => recolor_panel(o, &panel, p),
                    None => [o[0] as f32, o[1] as f32, o[2] as f32],
                }
            };
            let col = if !in_box {
                left
            } else {
                let a = art.get_pixel(x - ax, y - ay);
                let t = if edge {
                    ((x - ax) as f32 / fade as f32).min(1.0)
                } else {
                    rounded(x as f32 + 0.5, y as f32 + 0.5, x0 as f32, y0 as f32, x1 as f32 + 1.0, y1 as f32 + 1.0, radius)
                };
                let under = if edge { flat } else { match pal { Some(p) => recolor_panel(o, &panel, p), None => [o[0] as f32, o[1] as f32, o[2] as f32] } };
                [0, 1, 2].map(|i| under[i] + (a[i] as f32 - under[i]) * t)
            };
            out.put_pixel(x, y, Rgba([col[0].round() as u8, col[1].round() as u8, col[2].round() as u8, o[3]]));
        }
    }
    out
}

fn encode_rgba(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, img.width(), img.height());
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.set_compression(png::Compression::Best);
        e.set_filter(png::FilterType::Paeth);
        e.set_adaptive_filter(png::AdaptiveFilterType::Adaptive);
        let mut w = e.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(img.as_raw()).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// 256 colores (NeuQuant): mucho más pequeño, para cuando la imagen a todo color no cabe.
fn encode_indexed(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let nq = color_quant::NeuQuant::new(10, 256, img.as_raw());
    let pal = nq.color_map_rgba();
    let idx: Vec<u8> = img.as_raw().chunks_exact(4).map(|p| nq.index_of(p) as u8).collect();
    let rgb: Vec<u8> = pal.chunks_exact(4).flat_map(|c| [c[0], c[1], c[2]]).collect();
    let alpha: Vec<u8> = pal.chunks_exact(4).map(|c| c[3]).collect();
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, img.width(), img.height());
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        e.set_palette(rgb);
        if alpha.iter().any(|&a| a != 255) {
            e.set_trns(alpha);
        }
        e.set_compression(png::Compression::Best);
        let mut w = e.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&idx).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Rellena el PNG hasta `len` bytes con un bloque auxiliar privado ("atPd") justo antes de IEND.
fn pad(png_bytes: Vec<u8>, len: usize) -> Option<Vec<u8>> {
    if png_bytes.len() == len {
        return Some(png_bytes);
    }
    if png_bytes.len() + 12 > len {
        return None;
    }
    let iend = png_bytes.len() - 12;
    let n = len - png_bytes.len() - 12;
    let mut chunk = Vec::with_capacity(n + 12);
    chunk.extend_from_slice(&(n as u32).to_be_bytes());
    chunk.extend_from_slice(b"atPd");
    chunk.extend(std::iter::repeat(0u8).take(n));
    let crc = crc32(&chunk[4..]);
    chunk.extend_from_slice(&crc.to_be_bytes());
    let mut out = png_bytes[..iend].to_vec();
    out.extend(chunk);
    out.extend_from_slice(&png_bytes[iend..]);
    Some(out)
}

/// Escribe la imagen en los tres splash de `b` (una copia de AfterFXLib.dll, con los splash originales).
/// `res`: sus recursos. `pal`: la paleta para el panel y el logo (None = se dejan como están).
pub fn apply(b: &mut [u8], res: &[Resource], img: &DynamicImage, pal: Option<&Pal>) -> Result<usize, String> {
    let mut done = 0;
    for name in NAMES {
        let Some(r) = res.iter().find(|r| r.is("PNG", name)) else { continue };
        let slot = &b[r.off..r.off + r.size];
        png_size(slot).ok_or_else(|| tr!("{} isn't a PNG", "{} no es un PNG", name))?;
        let orig = image::load_from_memory(slot).map_err(|e| format!("{name}: {e}"))?.to_rgba8();
        let pic = compose(&orig, img, pal);
        let full = encode_rgba(&pic)?;
        let bytes = match pad(full, r.size) {
            Some(x) => x,
            None => pad(encode_indexed(&pic)?, r.size).ok_or_else(|| tr!("the image is too detailed to fit in {}", "la imagen tiene demasiado detalle para caber en {}", name))?,
        };
        b[r.off..r.off + r.size].copy_from_slice(&bytes);
        done += 1;
    }
    Ok(done)
}

/// Carga la imagen elegida (PNG o JPG).
pub fn load(path: &std::path::Path) -> Result<DynamicImage, String> {
    image::open(path).map_err(|e| tr!("couldn't open the image: {}", "no se pudo abrir la imagen: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_to_exact_size_and_stays_valid() {
        let img = RgbaImage::from_fn(64, 40, |x, y| image::Rgba([(x * 4) as u8, (y * 6) as u8, 120, 255]));
        let png_bytes = encode_rgba(&img).unwrap();
        let target = png_bytes.len() + 500;
        let out = pad(png_bytes, target).unwrap();
        assert_eq!(out.len(), target);
        let back = image::load_from_memory(&out).unwrap();
        assert_eq!((back.width(), back.height()), (64, 40));
    }

    #[test]
    fn cover_has_exact_size() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(300, 100));
        let c = cover(&img, 120, 90);
        assert_eq!((c.width(), c.height()), (120, 90));
    }
}
