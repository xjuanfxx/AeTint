//! Diálogos de Windows para abrir y guardar archivos (temas .aetint, imagen del splash).

use std::path::PathBuf;

/// Filtro: pares (descripción, patrón), p. ej. ("AeTint theme", "*.aetint").
#[cfg(windows)]
fn filter(items: &[(&str, &str)]) -> Vec<u16> {
    let mut v = Vec::new();
    for (d, p) in items {
        v.extend(d.encode_utf16());
        v.push(0);
        v.extend(p.encode_utf16());
        v.push(0);
    }
    v.push(0);
    v
}

#[cfg(windows)]
fn run(save: bool, title: &str, items: &[(&str, &str)], default_name: &str, ext: &str) -> Option<PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::*;
    let f = filter(items);
    let t: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let e: Vec<u16> = ext.encode_utf16().chain(Some(0)).collect();
    let mut buf = vec![0u16; 1024];
    for (i, c) in default_name.encode_utf16().take(1000).enumerate() {
        buf[i] = c;
    }
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = f.as_ptr();
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.lpstrTitle = t.as_ptr();
    ofn.lpstrDefExt = e.as_ptr();
    ofn.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | if save { OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST } else { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
    if ok == 0 {
        return None;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

#[cfg(windows)]
pub fn open(title: &str, items: &[(&str, &str)]) -> Option<PathBuf> {
    run(false, title, items, "", "")
}

#[cfg(windows)]
pub fn save(title: &str, items: &[(&str, &str)], default_name: &str, ext: &str) -> Option<PathBuf> {
    run(true, title, items, default_name, ext)
}

#[cfg(not(windows))]
pub fn open(_: &str, _: &[(&str, &str)]) -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
pub fn save(_: &str, _: &[(&str, &str)], _: &str, _: &str) -> Option<PathBuf> {
    None
}
