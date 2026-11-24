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

/// Recorta y escala la imagen para cubrir w × h. El recorte se centra en `focus` (0..1 en cada eje, dentro de la
/// imagen); sin punto elegido, en la zona con más interés para esa proporción (`auto_focus`).
fn cover(img: &DynamicImage, w: u32, h: u32, focus: Option<(f32, f32)>) -> RgbaImage {
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let s = (w as f64 / iw).max(h as f64 / ih);
    let (cw, ch) = ((w as f64 / s).round().max(1.0) as u32, (h as f64 / s).round().max(1.0) as u32);
    let (fx, fy) = focus.unwrap_or_else(|| auto_focus(img, w as f32 / h as f32));
    let place = |f: f32, full: f64, part: u32| -> u32 { ((f as f64 * full - part as f64 / 2.0).round().max(0.0) as u32).min((full as u32).saturating_sub(part)) };
    let (x, y) = (place(fx, iw, cw), place(fy, ih, ch));
    img.crop_imm(x, y, cw.min(iw as u32), ch.min(ih as u32)).resize_exact(w, h, FilterType::Lanczos3).to_rgba8()
}

/// Mapa de interés de la imagen en pequeño: bordes (cambios de luz), contraste local y color. Lo que más destaca
/// suele ser el sujeto (una cara, un personaje, un objeto) frente a cielos, fondos lisos o desenfocados.
fn interest(img: &DynamicImage) -> (Vec<f32>, u32, u32) {
    let small = img.thumbnail(192, 192).to_rgb8();
    let (w, h) = small.dimensions();
    let lum: Vec<f32> = small.pixels().map(|p| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).collect();
    let mut e = vec![0f32; (w * h) as usize];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            let gx = lum[i + 1] - lum[i - 1];
            let gy = lum[i + w as usize] - lum[i - w as usize];
            let p = small.get_pixel(x, y);
            let (mx, mn) = (p[0].max(p[1]).max(p[2]) as f32, p[0].min(p[1]).min(p[2]) as f32);
            let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
            e[i] = (gx * gx + gy * gy).sqrt() + 40.0 * sat;
        }
    }
    (e, w, h)
}

/// Punto donde centrar un recorte de proporción `aspect` (ancho / alto): la ventana que abarca más interés, con
/// una ligera preferencia por el centro. Devuelve (x, y) entre 0 y 1.
pub fn auto_focus(img: &DynamicImage, aspect: f32) -> (f32, f32) {
    let (e, w, h) = interest(img);
    if w < 4 || h < 4 {
        return (0.5, 0.5);
    }
    let ia = w as f32 / h as f32;
    // la ventana ocupa todo un eje y se desliza por el otro
    let horizontal = ia > aspect;
    let (len, span) = if horizontal { (w, ((h as f32 * aspect).round() as u32).clamp(1, w)) } else { (h, ((w as f32 / aspect).round() as u32).clamp(1, h)) };
    if span >= len {
        return (0.5, 0.5);
    }
    // interés por columna (o fila) y sumas acumuladas
    let mut line = vec![0f32; len as usize];
    for y in 0..h {
        for x in 0..w {
            line[(if horizontal { x } else { y }) as usize] += e[(y * w + x) as usize];
        }
    }
    let mut acc = vec![0f32; len as usize + 1];
    for i in 0..len as usize {
        acc[i + 1] = acc[i] + line[i];
    }
    let total = acc[len as usize].max(1.0);
    let (mut best, mut best_at) = (f32::MIN, (len - span) / 2);
    for at in 0..=(len - span) {
        let sum = (acc[(at + span) as usize] - acc[at as usize]) / total;
        let off = ((at as f32 + span as f32 / 2.0) / len as f32 - 0.5).abs();
        let score = sum - 0.08 * off;
        if score > best {
            (best, best_at) = (score, at);
        }
    }
    let c = (best_at as f32 + span as f32 / 2.0) / len as f32;
    if horizontal { (c, 0.5) } else { (0.5, c) }
}

fn diff(a: &Rgba<u8>, b: &Rgba<u8>) -> u32 {
    (0..3).map(|i| (a[i] as i32 - b[i] as i32).unsigned_abs()).sum()
}

/// Rectángulo de la ilustración (x0, y0, x1, y1, inclusive): lo que deja de ser el panel liso, buscando desde
/// un cuarto del ancho. En 2025+ llega a los bordes; en 2024 es un recuadro redondeado con márgenes.
/// Se salta una franja del borde de la tarjeta (1 %): en 2023 la tarjeta lleva un filo gris de 1 px que, si
/// contara, haría creer que la ilustración llega hasta las orillas.
fn art_box(orig: &RgbaImage, panel: &Rgba<u8>) -> (u32, u32, u32, u32) {
    let (w, h) = orig.dimensions();
    let band = (w / 100).max(2);
    // una columna (o fila) es ilustración si más de un tercio deja de ser el panel: el texto del logo ocupa poco
    let col = |x: u32| {
        let ys: Vec<u32> = (band..h - band).step_by(3).collect();
        ys.iter().filter(|&&y| diff(orig.get_pixel(x, y), panel) > 24).count() as f32 / ys.len() as f32
    };
    let x0 = ((w / 4)..w - band).find(|&x| col(x) > 0.34).unwrap_or((w as f32 * 0.43) as u32);
    let x1 = (x0..w - band).rev().find(|&x| col(x) > 0.34).unwrap_or(w - 1);
    let row = |y: u32| {
        let xs: Vec<u32> = (x0..=x1).step_by(3).collect();
        xs.iter().filter(|&&x| diff(orig.get_pixel(x, y), panel) > 24).count() as f32 / xs.len() as f32
    };
    let y0 = (band..h - band).find(|&y| row(y) > 0.34).unwrap_or(0);
    let y1 = (y0..h - band).rev().find(|&y| row(y) > 0.34).unwrap_or(h - 1);
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
    // la saturación del color de la letra, no la del píxel: en el borde suavizado la letra está mezclada con el
    // panel y parece menos saturada; si se mirara el píxel, el filo de una letra de color pasaba al color del texto
    let fg0: [f32; 3] = [0, 1, 2].map(|i| (panel[i] as f32 + (c[i] as f32 - panel[i] as f32) / a.max(0.08)).clamp(0.0, 255.0));
    let (mx, mn) = (fg0[0].max(fg0[1]).max(fg0[2]), fg0[0].min(fg0[1]).min(fg0[2]));
    let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
    let fg = to8(if sat > 0.35 { pal.accent } else { pal.text });
    let bg = to8(pal.panel);
    [0, 1, 2].map(|i| bg[i] + (fg[i] - bg[i]) * a)
}

fn saturation(c: &Rgba<u8>) -> f32 {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]) as f32, c[0].min(c[1]).min(c[2]) as f32);
    if mx > 0.0 { (mx - mn) / mx } else { 0.0 }
}

/// Iconos del panel (la caja "Ae", el de Creative Cloud): manchas saturadas que llenan casi todo su recuadro.
/// Devuelve su recuadro (x0, y0, x1, y1) y el color de la caja. Con el recolor normal la caja y sus letras
/// acabarían del mismo tono; así la caja va con el acento y las letras con el color del panel.
fn icons(orig: &RgbaImage, panel: &Rgba<u8>, right: u32) -> Vec<(u32, u32, u32, u32, Rgba<u8>)> {
    let (w, h) = orig.dimensions();
    let right = right.min(w);
    let on = |x: u32, y: u32| {
        let p = orig.get_pixel(x, y);
        p[3] == 255 && diff(p, panel) > 60 && saturation(p) > 0.35
    };
    let mut seen = vec![false; (w * h) as usize];
    let mut out = Vec::new();
    for sy in 0..h {
        for sx in 0..right {
            if seen[(sy * w + sx) as usize] || !on(sx, sy) {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1, mut n) = (sx, sy, sx, sy, 0u32);
            let mut stack = vec![(sx, sy)];
            seen[(sy * w + sx) as usize] = true;
            while let Some((x, y)) = stack.pop() {
                n += 1;
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                    if nx < right && ny < h && !seen[(ny * w + nx) as usize] && on(nx, ny) {
                        seen[(ny * w + nx) as usize] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
            let min = (w as f32 * 0.02) as u32;
            let square = bw.max(bh) as f32 / bw.min(bh) as f32 <= 1.3;
            if bw >= min && bh >= min && square && n as f32 >= 0.8 * (bw * bh) as f32 {
                // el color de la caja: cerca del borde izquierdo, a media altura (fuera de las letras)
                let boxc = *orig.get_pixel(x0 + bw / 12, y0 + bh / 2);
                out.push((x0, y0, x1, y1, boxc));
            }
        }
    }
    out
}

/// Un píxel de icono: entre el acento (la caja) y el color del panel (letras y filo suavizado), según lo cerca
/// que estaba de la caja o del panel original.
fn recolor_icon(c: &Rgba<u8>, boxc: &Rgba<u8>, panel: &Rgba<u8>, pal: &Pal) -> [f32; 3] {
    let (db, dp) = (diff(c, boxc) as f32, diff(c, panel) as f32);
    let t = if db + dp > 0.0 { db / (db + dp) } else { 0.0 };
    let t = ((t - 0.15) / 0.55).clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    let (fg, bg) = (to8(pal.accent), to8(pal.panel));
    [0, 1, 2].map(|i| fg[i] + (bg[i] - fg[i]) * t)
}

/// Curva suave (smootherstep): sin pendiente en los extremos, así no se ve dónde empieza ni dónde acaba el fundido.
fn smoother(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Ruido de ±0.5 niveles por píxel (hash entero), contra las bandas del degradado.
fn dither(x: u32, y: u32) -> f32 {
    let mut v = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 12;
    (v & 0xFFFF) as f32 / 65535.0 - 0.5
}

/// Columnas donde empieza y acaba el fundido: desde el hueco que queda a la derecha del logo (con un margen) hasta
/// bien dentro de la ilustración, para que la imagen se funda con el panel en un tramo largo.
fn fade_span(orig: &RgbaImage, panel: &Rgba<u8>, x0: u32) -> (u32, u32) {
    let (w, h) = orig.dimensions();
    // solo píxeles opacos: las esquinas redondeadas transparentes no cuentan
    let any = |x: u32| (0..h).step_by(2).any(|y| { let p = orig.get_pixel(x, y); p[3] == 255 && diff(p, panel) > 12 });
    // la cola del fundido original de Adobe, luego el hueco vacío, y lo que quede a la izquierda es el logo
    let mut x = x0;
    while x > 0 && any(x - 1) {
        x -= 1;
    }
    let tail = x;
    while x > 0 && !any(x - 1) {
        x -= 1;
    }
    let text_right = x;
    let wf = w as f32;
    if tail < w / 4 {
        // no se distingue el hueco (panel con textura): un fundido prudente que no llegue al logo
        return (x0.saturating_sub((wf * 0.05) as u32), (x0 + (wf * 0.12) as u32).min(w - 1));
    }
    let start = (text_right + (wf * 0.03) as u32).max(x0.saturating_sub((wf * 0.14) as u32)).min(tail.min(x0));
    let end = (x0 + (wf * 0.16) as u32).min(w - 1).max(start + 1);
    (start, end)
}

/// La imagen del usuario en la zona de la ilustración, el panel con la paleta y las esquinas como estaban.
fn compose(orig: &RgbaImage, user: &DynamicImage, pal: Option<&Pal>, fade: bool, focus: Option<(f32, f32)>) -> RgbaImage {
    let (w, h) = orig.dimensions();
    let panel = *orig.get_pixel((w as f32 * 0.03) as u32, (h as f32 * 0.6) as u32);
    let (x0, y0, x1, y1) = art_box(orig, &panel);
    // ¿la ilustración llega a los bordes (fundido por la izquierda) o es un recuadro con márgenes?
    let band = (w / 100).max(2);
    let edge = x1 + band + 3 >= w && y0 <= band + 3 && y1 + band + 3 >= h;
    // fundido ancho y suave: empieza en el hueco entre el logo y la ilustración y entra en la imagen
    let (fs, fe) = fade_span(orig, &panel, x0);
    // degradado: la imagen ocupa todo el lado derecho y se funde con el panel. Recuadro: la imagen en un
    // rectángulo redondeado con margen. En 2024 y anteriores es el de Adobe; en 2025+ la ilustración de Adobe llega
    // a los bordes, así que se arma uno con el mismo margen por los tres lados, empezando tras el texto del logo.
    let (x0, y0, x1, y1) = if edge && !fade {
        // dónde empieza la tarjeta opaca (las esquinas y la sombra de alrededor son transparentes)
        let opaque = |x: u32, y: u32| orig.get_pixel(x, y)[3] == 255;
        let top = (0..h / 2).find(|&y| opaque(w * 3 / 4, y)).unwrap_or(0);
        let bottom = (h / 2..h).rev().find(|&y| opaque(w * 3 / 4, y)).unwrap_or(h - 1);
        let right = (w / 2..w).rev().find(|&x| opaque(x, h / 2)).unwrap_or(w - 1);
        let m = (w as f32 * 0.02) as u32;
        // empieza donde empezaba la ilustración de Adobe: a la izquierda AE escribe al arrancar el aviso legal
        // y el progreso, que llegan casi al 40 % del ancho
        let left = x0.max((w as f32 * 0.42) as u32);
        (left, top + m, right.saturating_sub(m), bottom.saturating_sub(m))
    } else {
        (x0, y0, x1, y1)
    };
    let (ax, ay) = if fade { (fs, 0) } else { (x0, y0) };
    let (aw, ah) = if fade { (w - ax, h) } else { (x1 - x0 + 1, y1 - y0 + 1) };
    let art = cover(user, aw, ah, focus);
    // tarjeta con filo (2024 y anteriores): en modo degradado el filo se conserva encima de la imagen
    let rim = if fade && !edge { (w / 500).max(1) } else { 0 };
    let near_rim = |x: u32, y: u32| {
        (1..=rim).any(|k| {
            let t = |xx: i64, yy: i64| xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 || orig.get_pixel(xx as u32, yy as u32)[3] < 255;
            let (x, y, k) = (x as i64, y as i64, k as i64);
            t(x - k, y) || t(x + k, y) || t(x, y - k) || t(x, y + k)
        })
    };
    let radius = w as f32 * 0.027;
    let flat = match pal {
        Some(p) => to8(p.panel),
        None => [panel[0] as f32, panel[1] as f32, panel[2] as f32],
    };
    let marks = if pal.is_some() { icons(orig, &panel, ax) } else { Vec::new() };
    let mut out = orig.clone();
    for y in 0..h {
        for x in 0..w {
            let o = orig.get_pixel(x, y);
            let in_box = x >= ax && x < ax + aw && y >= ay && y < ay + ah;
            // fuera de la ilustración: el panel (y el logo) con la paleta
            // recuadro armado (2025+): a la derecha del texto estaba la ilustración de Adobe; ahí va el panel liso
            let left = if in_box || (edge && !fade && x >= fs) {
                flat
            } else {
                match pal {
                    Some(p) => match marks.iter().find(|m| x >= m.0 && x <= m.2 && y >= m.1 && y <= m.3) {
                        Some(m) => recolor_icon(o, &m.4, &panel, p),
                        None => recolor_panel(o, &panel, p),
                    },
                    None => [o[0] as f32, o[1] as f32, o[2] as f32],
                }
            };
            let col = if !in_box {
                left
            } else if rim > 0 && (o[3] < 255 || near_rim(x, y)) {
                // el filo de la tarjeta, con la paleta: junto al panel se ve y sobre la imagen se desvanece con el
                // mismo degradado (si se quedara entero, era una línea oscura alrededor de la imagen)
                let edge_col = match pal {
                    Some(p) => recolor_panel(o, &panel, p),
                    None => [o[0] as f32, o[1] as f32, o[2] as f32],
                };
                let u = ((x as f32 + 0.5 - fs as f32) / (fe - fs).max(1) as f32).clamp(0.0, 1.0);
                let t = smoother(u);
                let p = art.get_pixel(x - ax, y - ay);
                [0, 1, 2].map(|i| edge_col[i] + (p[i] as f32 - edge_col[i]) * t)
            } else {
                let u = ((x as f32 + 0.5 - fs as f32) / (fe - fs).max(1) as f32).clamp(0.0, 1.0);
                let p = art.get_pixel(x - ax, y - ay);
                let a = [p[0] as f32, p[1] as f32, p[2] as f32];
                let t = if fade {
                    // fundido limpio (sin copia desenfocada: los brillos se volvían manchas borrosas)
                    smoother(u)
                } else {
                    rounded(x as f32 + 0.5, y as f32 + 0.5, x0 as f32, y0 as f32, x1 as f32 + 1.0, y1 as f32 + 1.0, radius)
                };
                // debajo, el panel liso: en las esquinas del recuadro el original tiene el filo de la ilustración de
                // Adobe, y recolorearlo dejaba otro color asomando por la curva
                let under = flat;
                // un poco de ruido donde mezcla, para que el degradado no haga escalones
                let n = if t > 0.0 && t < 1.0 { dither(x, y) } else { 0.0 };
                [0, 1, 2].map(|i| under[i] + (a[i] - under[i]) * t + n)
            };
            // los píxeles transparentes (esquinas) con el color del panel: AE escala el splash sin premultiplicar, y el
            // color que tengan se cuela por el borde (con el del texto se veía un recuadro claro detrás de la curva)
            let col = if o[3] == 0 { flat } else { col };
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
pub fn apply(b: &mut [u8], res: &[Resource], img: &DynamicImage, pal: Option<&Pal>, fade: bool, focus: Option<(f32, f32)>) -> Result<usize, String> {
    let mut done = 0;
    for name in NAMES {
        let Some(r) = res.iter().find(|r| r.is("PNG", name)) else { continue };
        let slot = &b[r.off..r.off + r.size];
        png_size(slot).ok_or_else(|| tr!("{} isn't a PNG", "{} no es un PNG", name))?;
        let orig = image::load_from_memory(slot).map_err(|e| format!("{name}: {e}"))?.to_rgba8();
        let pic = compose(&orig, img, pal, fade, focus);
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

    /// `AETINT_SPLASH_LIB=…\AfterFXLib.dll AETINT_SPLASH_IMG=foto.jpg AETINT_SPLASH_OUT=carpeta cargo test splash_render -- --ignored`
    /// Guarda cada splash original, el compuesto y el que quedaría escrito (tras comprimir para que quepa).
    #[test]
    #[ignore]
    fn splash_render() {
        let lib = std::fs::read(std::env::var("AETINT_SPLASH_LIB").unwrap()).unwrap();
        let img = load(std::path::Path::new(&std::env::var("AETINT_SPLASH_IMG").unwrap())).unwrap();
        let out = std::path::PathBuf::from(std::env::var("AETINT_SPLASH_OUT").unwrap());
        let res = crate::pe::Pe::parse(&lib).unwrap().resources(&lib);
        let pal = crate::remap::Palette {
            base: "#11111B".into(), panel: "#1E1E2E".into(), frame: "#313244".into(),
            raised: "#181825".into(), text: "#CDD6F4".into(), accent: "#7A80C2".into(),
        }.resolve().unwrap();
        for name in NAMES {
            let Some(r) = res.iter().find(|r| r.is("PNG", name)) else { continue };
            let slot = &lib[r.off..r.off + r.size];
            let orig = image::load_from_memory(slot).unwrap().to_rgba8();
            orig.save(out.join(format!("{name}-orig.png"))).unwrap();
            let pic = compose(&orig, &img, Some(&pal), std::env::var_os("AETINT_SPLASH_BOX").is_none(), None);
            pic.save(out.join(format!("{name}-compose.png"))).unwrap();
            let full = encode_rgba(&pic).unwrap();
            let fits = full.len() <= r.size;
            let bytes = if fits { full } else { encode_indexed(&pic).unwrap() };
            std::fs::write(out.join(format!("{name}-final.png")), &bytes).unwrap();
            println!("{name}: {}x{} slot {} full fits {fits} final {}", orig.width(), orig.height(), r.size, bytes.len());
        }
    }

    #[test]
    fn cover_has_exact_size() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(300, 100));
        let c = cover(&img, 120, 90, None);
        assert_eq!((c.width(), c.height()), (120, 90));
    }
}
