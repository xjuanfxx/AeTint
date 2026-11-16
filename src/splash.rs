//! Pantalla de inicio (splash) propia: la imagen del usuario se escribe en los tres PNG de AfterFXLib.dll
//! ("AE_SPLASH", "AE_SPLASH_AT_3TO2X" y "AE_SPLASH_AT_2X"), en su sitio y sin cambiar el tamaño del archivo:
//! se recorta al tamaño exacto de cada uno, se comprime y el hueco que sobra se rellena con un bloque PNG
//! que los lectores ignoran. Si con todos los colores no cabe, se reduce a 256 colores.
//! "Restaurar" lo deshace con el resto del archivo.

use crate::pe::Resource;
use image::{imageops::FilterType, DynamicImage, RgbaImage};

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

/// Escribe la imagen en los tres splash de `b` (una copia de AfterFXLib.dll). `res`: sus recursos.
pub fn apply(b: &mut [u8], res: &[Resource], img: &DynamicImage) -> Result<usize, String> {
    let mut done = 0;
    for name in NAMES {
        let Some(r) = res.iter().find(|r| r.is("PNG", name)) else { continue };
        let slot = &b[r.off..r.off + r.size];
        let (w, h) = png_size(slot).ok_or_else(|| tr!("{} isn't a PNG", "{} no es un PNG", name))?;
        let pic = cover(img, w, h);
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
