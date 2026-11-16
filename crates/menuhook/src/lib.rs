//! aetint_menu.dll — se carga dentro de After Effects y pinta con la paleta de AeTint lo que dibuja Windows:
//!   · la barra de menú (File, Edit, …): mensajes no documentados WM_UAHDRAWMENU / WM_UAHDRAWMENUITEM,
//!     los mismos que usa el modo oscuro de Notepad++;
//!   · los menús desplegables: se intercepta el dibujo de la clase "Menu" del tema de Windows
//!     (DrawThemeBackground / DrawThemeText / DrawThemeTextEx);
//!   · la barra de título: DwmSetWindowAttribute (colores de título, texto y borde, Windows 11).
//! Los colores se leen de %LOCALAPPDATA%\AeTint\menu.json y se recargan en vivo si el archivo cambia.
//! No modifica ningún archivo de Adobe.

#![allow(clippy::missing_safety_doc)]

use retour::RawDetour;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, SystemTime};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{DisableThreadLibraryCalls, GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODT_MENU, ODS_CHECKED, ODS_DISABLED, ODS_GRAYED, ODS_HOTLIGHT, ODS_INACTIVE, ODS_NOACCEL, ODS_SELECTED};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SystemParametersInfoForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ── colores

#[derive(Deserialize, Clone)]
struct Config {
    #[serde(default = "yes")]
    enabled: bool,
    bar: String,
    bar_hover: String,
    text: String,
    text_disabled: String,
    #[serde(default)]
    text_hover: Option<String>,
    popup: String,
    popup_hover: String,
    border: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    title_text: Option<String>,
    /// colores para el brillo claro de AE (Light)
    #[serde(default)]
    light: Option<Box<Config>>,
}
fn yes() -> bool {
    true
}

#[derive(Clone, Copy)]
struct Colors {
    enabled: bool,
    bar: u32,
    bar_hover: u32,
    text: u32,
    text_disabled: u32,
    text_hover: u32,
    popup: u32,
    popup_hover: u32,
    border: u32,
    title: Option<u32>,
    title_text: Option<u32>,
}

fn colorref(h: &str) -> Option<u32> {
    let h = h.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    let (r, g, b) = ((n >> 16) & 255, (n >> 8) & 255, n & 255);
    Some(r | (g << 8) | (b << 16))
}

impl Colors {
    fn from(c: &Config) -> Option<Colors> {
        Some(Colors {
            enabled: c.enabled,
            bar: colorref(&c.bar)?,
            bar_hover: colorref(&c.bar_hover)?,
            text: colorref(&c.text)?,
            text_disabled: colorref(&c.text_disabled)?,
            text_hover: c.text_hover.as_deref().and_then(colorref).unwrap_or(colorref(&c.text)?),
            popup: colorref(&c.popup)?,
            popup_hover: colorref(&c.popup_hover)?,
            border: colorref(&c.border)?,
            title: c.title.as_deref().and_then(colorref),
            title_text: c.title_text.as_deref().and_then(colorref),
        })
    }
}

/// (colores oscuros, colores claros)
static COLORS: RwLock<Option<(Colors, Option<Colors>)>> = RwLock::new(None);

/// Los colores del menú oscuro de Windows 11 (para opciones owner-draw cuando no hay tema).
fn native_colors() -> Colors {
    let c = |h: &str| colorref(h).unwrap_or(0);
    Colors {
        enabled: true,
        bar: c("#202020"),
        bar_hover: c("#2D2D2D"),
        text: c("#FFFFFF"),
        text_disabled: c("#6D6D6D"),
        text_hover: c("#FFFFFF"),
        popup: c("#2C2C2C"),
        popup_hover: c("#414141"),
        border: c("#3E3E3E"),
        title: None,
        title_text: None,
    }
}
/// After Effects está en su brillo claro (Light)
static LIGHT_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn colors() -> Option<Colors> {
    let (dark, light) = COLORS.read().ok().and_then(|c| *c)?;
    if !dark.enabled {
        return None;
    }
    if LIGHT_MODE.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(l) = light {
            return Some(l);
        }
    }
    Some(dark)
}

/// Archivo de preferencias de la versión de AE que está corriendo:
/// %APPDATA%\Adobe\After Effects\<mayor>.<menor>\Adobe After Effects <mayor>.<menor> Prefs-indep-general.txt
fn ae_prefs_path() -> Option<std::path::PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO};
    unsafe {
        let mut buf = [0u16; 1024];
        let n = windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) as usize;
        let exe = &buf[..=n.min(buf.len() - 1)];
        let size = GetFileVersionInfoSizeW(exe.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(exe.as_ptr(), 0, size, data.as_mut_ptr() as *mut c_void) == 0 {
            return None;
        }
        let mut p: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if VerQueryValueW(data.as_ptr() as *const c_void, wide("\\").as_ptr(), &mut p, &mut len) == 0 || p.is_null() {
            return None;
        }
        let fi = &*(p as *const VS_FIXEDFILEINFO);
        let ver = format!("{}.{}", fi.dwFileVersionMS >> 16, fi.dwFileVersionMS & 0xffff);
        let appdata = std::env::var_os("APPDATA")?;
        Some(std::path::PathBuf::from(appdata).join("Adobe").join("After Effects").join(&ver).join(format!("Adobe After Effects {ver} Prefs-indep-general.txt")))
    }
}

/// Brillo de la interfaz que eligió el usuario en AE (0 Darkest … 1 Light).
fn ae_brightness(p: &std::path::Path) -> Option<f32> {
    let s = std::fs::read_to_string(p).ok()?;
    let line = s.lines().find(|l| l.contains("User Interface Brightness"))?;
    let v = line.rsplit('=').next()?.trim().trim_matches('"');
    v.parse().ok()
}

/// Cambia de juego de colores cuando AE cambia de brillo; devuelve true si cambió.
fn set_light_mode(light: bool) -> bool {
    let was = LIGHT_MODE.swap(light, std::sync::atomic::Ordering::Relaxed);
    if was != light {
        log(&format!("brillo de AE: {}", if light { "claro" } else { "oscuro" }));
    }
    was != light
}

fn config_path() -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_default();
    base.join("AeTint").join("menu.json")
}

// ── utilidades GDI

unsafe fn fill(hdc: HDC, rc: &RECT, c: u32) {
    let b = CreateSolidBrush(c);
    FillRect(hdc, rc, b);
    DeleteObject(b as HGDIOBJ);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Fuente del menú para el DPI de la ventana (cacheada por DPI).
unsafe fn menu_font(hwnd: HWND) -> HFONT {
    static FONTS: Mutex<Vec<(u32, isize)>> = Mutex::new(Vec::new());
    let dpi = GetDpiForWindow(hwnd).max(96);
    let mut f = FONTS.lock().unwrap();
    if let Some((_, h)) = f.iter().find(|(d, _)| *d == dpi) {
        return *h as HFONT;
    }
    let mut ncm: NONCLIENTMETRICSW = std::mem::zeroed();
    ncm.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
    SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut c_void, 0, dpi);
    let h = CreateFontIndirectW(&ncm.lfMenuFont);
    f.push((dpi, h as isize));
    h
}

// ── barra de menú (UAH)

const WM_UAHDRAWMENU: u32 = 0x0091;
const WM_UAHDRAWMENUITEM: u32 = 0x0092;

#[repr(C)]
struct UahMenu {
    hmenu: HMENU,
    hdc: HDC,
    flags: u32,
}

#[repr(C)]
struct UahMenuItem {
    position: i32,
    metrics: [u32; 8],
    flags: u32,
}

#[repr(C)]
struct UahDrawMenuItem {
    dis: DRAWITEMSTRUCT,
    um: UahMenu,
    umi: UahMenuItem,
}

static OLD_PROCS: Mutex<Option<HashMap<isize, isize>>> = Mutex::new(None);

fn old_proc(hwnd: HWND) -> Option<isize> {
    OLD_PROCS.lock().ok()?.as_ref()?.get(&(hwnd as isize)).copied()
}

/// La barra de menú en coordenadas de ventana.
unsafe fn bar_rect(hwnd: HWND) -> Option<RECT> {
    let mut mbi: MENUBARINFO = std::mem::zeroed();
    mbi.cbSize = std::mem::size_of::<MENUBARINFO>() as u32;
    if GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut mbi) == 0 {
        return None;
    }
    let mut wr: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut wr);
    let mut r = mbi.rcBar;
    r.left -= wr.left;
    r.right -= wr.left;
    r.top -= wr.top;
    r.bottom -= wr.top;
    Some(r)
}

/// La línea de 1 px que Windows deja debajo de la barra de menú (clara en modo claro).
unsafe fn paint_bar_line(hwnd: HWND, c: &Colors) {
    let Some(mut r) = bar_rect(hwnd) else { return };
    let mut cr: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut cr);
    let mut wr: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut wr);
    let mut pt = POINT { x: 0, y: 0 };
    ClientToScreen(hwnd, &mut pt);
    let client_top = pt.y - wr.top;
    r.top = r.bottom;
    r.bottom = client_top.max(r.top + 1);
    let hdc = GetWindowDC(hwnd);
    fill(hdc, &r, c.bar);
    ReleaseDC(hwnd, hdc);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(old) = old_proc(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    let call_old = |m, w, l| CallWindowProcW(Some(std::mem::transmute::<isize, WndProcFn>(old)), hwnd, m, w, l);
    let Some(c) = colors() else {
        // pintado desactivado (o tema original de AE) con AE abierto: las opciones que ya pasamos a owner-draw
        // se siguen dibujando, con los colores del menú oscuro de Windows, hasta reiniciar AE
        match msg {
            WM_MEASUREITEM => {
                let mis = &mut *(lp as *mut MEASUREITEMSTRUCT);
                if let Some(it) = od_item(mis.itemData).filter(|_| mis.CtlType == ODT_MENU) {
                    od_measure(hwnd, mis, it);
                    return 1;
                }
            }
            WM_DRAWITEM => {
                let d = &*(lp as *const DRAWITEMSTRUCT);
                if let Some(it) = od_item(d.itemData).filter(|_| d.CtlType == ODT_MENU) {
                    od_draw(hwnd, d, it, &native_colors());
                    return 1;
                }
            }
            _ => {}
        }
        return call_old(msg, wp, lp);
    };
    match msg {
        WM_UAHDRAWMENU => {
            log("UAHDRAWMENU");
            let um = &*(lp as *const UahMenu);
            if let Some(r) = bar_rect(hwnd) {
                fill(um.hdc, &r, c.bar);
            }
            1
        }
        WM_UAHDRAWMENUITEM => {
            let it = &*(lp as *const UahDrawMenuItem);
            let dis = &it.dis;
            let st = dis.itemState;
            let hot = st & (ODS_HOTLIGHT | ODS_SELECTED) != 0;
            let disabled = st & (ODS_GRAYED | ODS_DISABLED | ODS_INACTIVE) != 0;
            fill(dis.hDC, &dis.rcItem, if hot { c.bar_hover } else { c.bar });

            let mut buf = [0u16; 256];
            let mut mii: MENUITEMINFOW = std::mem::zeroed();
            mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
            mii.fMask = MIIM_STRING;
            mii.dwTypeData = buf.as_mut_ptr();
            mii.cch = buf.len() as u32 - 1;
            if GetMenuItemInfoW(it.um.hmenu, it.umi.position as u32, 1, &mut mii) != 0 {
                let old_font = SelectObject(dis.hDC, menu_font(hwnd) as HGDIOBJ);
                SetBkMode(dis.hDC, TRANSPARENT as i32);
                SetTextColor(dis.hDC, if disabled { c.text_disabled } else { c.text });
                let mut flags = DT_CENTER | DT_SINGLELINE | DT_VCENTER;
                if st & ODS_NOACCEL != 0 {
                    flags |= DT_HIDEPREFIX;
                }
                let mut rc = dis.rcItem;
                DrawTextW(dis.hDC, buf.as_ptr(), mii.cch as i32, &mut rc, flags);
                SelectObject(dis.hDC, old_font);
            }
            1
        }
        WM_INITMENUPOPUP => {
            // AE rellena y activa las opciones primero; luego, si tiene columnas, pasa a owner-draw
            let r = call_old(msg, wp, lp);
            own_menu(wp as HMENU, &c, hwnd);
            r
        }
        WM_MEASUREITEM => {
            let mis = &mut *(lp as *mut MEASUREITEMSTRUCT);
            match od_item(mis.itemData) {
                Some(it) if mis.CtlType == ODT_MENU => {
                    od_measure(hwnd, mis, it);
                    1
                }
                _ => call_old(msg, wp, lp),
            }
        }
        WM_DRAWITEM => {
            let d = &*(lp as *const DRAWITEMSTRUCT);
            match od_item(d.itemData) {
                Some(it) if d.CtlType == ODT_MENU => {
                    od_draw(hwnd, d, it, &c);
                    1
                }
                _ => call_old(msg, wp, lp),
            }
        }
        WM_NCPAINT | WM_NCACTIVATE => {
            let r = call_old(msg, wp, lp);
            paint_bar_line(hwnd, &c);
            r
        }
        WM_NCDESTROY => {
            if let Ok(mut m) = OLD_PROCS.lock() {
                if let Some(m) = m.as_mut() {
                    m.remove(&(hwnd as isize));
                }
            }
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, old);
            call_old(msg, wp, lp)
        }
        _ => call_old(msg, wp, lp),
    }
}

type WndProcFn = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

unsafe fn subclass(hwnd: HWND) {
    let mut g = OLD_PROCS.lock().unwrap();
    let m = g.get_or_insert_with(HashMap::new);
    if m.contains_key(&(hwnd as isize)) {
        return;
    }
    let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wndproc as *const () as usize as isize);
    log(&format!("subclass {:#x} old {old:#x} err {}", hwnd as isize, GetLastError()));
    if old != 0 {
        m.insert(hwnd as isize, old);
    }
}

// ── opciones de los menús desplegables, pintadas por nosotros (owner-draw)
// · Si el tema del menú se abrió antes de cargar la DLL, Windows pinta las opciones por un camino interno
//   que no pasa por DrawThemeBackground: saldrían con el gris de Windows.
// · Los menús con columnas (Effect) los dibuja el kernel sin tema, con aspecto de Windows clásico.
// Por eso, al abrirse cada menú (WM_INITMENUPOPUP), sus opciones pasan a owner-draw.

const OD_MAGIC: u32 = 0xAE71_0D00;

struct OdItem {
    magic: u32,
    label: Vec<u16>,
    accel: Vec<u16>,
    sep: bool,
    sub: bool,
    /// menú al que pertenece (para la altura compacta de los menús con columnas)
    menu: isize,
}

/// Altura de opción por menú con columnas, calculada para que cada columna quepa en la pantalla.
static COL_HEIGHT: Mutex<Option<HashMap<isize, i32>>> = Mutex::new(None);

/// AE reparte las categorías en columnas pensando en la altura clásica de Windows; con opciones más
/// altas la columna se saldría de la pantalla. Se calcula la altura que hace caber la columna más larga.
unsafe fn fit_columns(m: HMENU, hwnd: HWND) {
    let mut cols: Vec<(i32, i32)> = vec![(0, 0)]; // (opciones, separadores) por columna
    for i in 0..GetMenuItemCount(m).max(0) {
        let mut mii: MENUITEMINFOW = std::mem::zeroed();
        mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
        mii.fMask = MIIM_FTYPE;
        if GetMenuItemInfoW(m, i as u32, 1, &mut mii) == 0 {
            continue;
        }
        if mii.fType & (MFT_MENUBREAK | MFT_MENUBARBREAK) != 0 && cols.last().map(|c| c.0 + c.1 > 0).unwrap_or(false) {
            cols.push((0, 0));
        }
        let c = cols.last_mut().unwrap();
        if mii.fType & MFT_SEPARATOR != 0 {
            c.1 += 1;
        } else {
            c.0 += 1;
        }
    }
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let sc = |v: i32| v * dpi / 96;
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    GetMonitorInfoW(MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST), &mut mi);
    let avail = (mi.rcWork.bottom - mi.rcWork.top) - sc(12);
    let dc = GetDC(std::ptr::null_mut());
    let old = SelectObject(dc, menu_font(hwnd) as HGDIOBJ);
    let (_, fh) = text_size(dc, &wide("Ag")[..2]);
    SelectObject(dc, old);
    ReleaseDC(std::ptr::null_mut(), dc);
    let normal = fh + sc(8);
    let mut h = normal;
    for (n, sp) in &cols {
        if *n > 0 {
            h = h.min((avail - sp * sc(7)) / n);
        }
    }
    let h = h.clamp(fh + sc(1), normal);
    log(&format!("menú {:#x}: {} columnas, opción de {h}px (normal {normal}px)", m as isize, cols.len()));
    COL_HEIGHT.lock().unwrap().get_or_insert_with(HashMap::new).insert(m as isize, h);
}

fn col_height(m: isize) -> Option<i32> {
    COL_HEIGHT.lock().ok()?.as_ref()?.get(&m).copied()
}

static OD_ITEMS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
/// Menús con columnas: además se pinta el borde de su ventana (que no tiene tema).
static OD_MENUS: Mutex<Option<HashSet<isize>>> = Mutex::new(None);
static CWP_THREADS: Mutex<Option<HashSet<u32>>> = Mutex::new(None);
static MODULE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn od_item(data: usize) -> Option<&'static OdItem> {
    if data == 0 || !OD_ITEMS.lock().ok()?.as_ref()?.contains(&data) {
        return None;
    }
    let it = unsafe { &*(data as *const OdItem) };
    (it.magic == OD_MAGIC).then_some(it)
}

fn is_od_menu(m: HMENU) -> bool {
    OD_MENUS.lock().ok().and_then(|g| g.as_ref().map(|s| s.contains(&(m as isize)))).unwrap_or(false)
}

unsafe fn popup_brush(c: u32) -> HBRUSH {
    static BR: Mutex<(u32, isize)> = Mutex::new((u32::MAX, 0));
    let mut g = BR.lock().unwrap();
    if g.0 != c {
        if g.1 != 0 {
            DeleteObject(g.1 as HGDIOBJ);
        }
        *g = (c, CreateSolidBrush(c) as isize);
    }
    g.1 as HBRUSH
}

unsafe fn has_columns(m: HMENU) -> bool {
    for i in 0..GetMenuItemCount(m).max(0) {
        let mut mii: MENUITEMINFOW = std::mem::zeroed();
        mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
        mii.fMask = MIIM_FTYPE;
        if GetMenuItemInfoW(m, i as u32, 1, &mut mii) != 0 && mii.fType & (MFT_MENUBREAK | MFT_MENUBARBREAK) != 0 {
            return true;
        }
    }
    false
}

unsafe fn own_menu(m: HMENU, c: &Colors, hwnd: HWND) {
    if m.is_null() || IsMenu(m) == 0 {
        return;
    }
    let cols = has_columns(m);
    if cols {
        fit_columns(m, hwnd);
        let mut mi: MENUINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MENUINFO>() as u32;
        mi.fMask = MIM_BACKGROUND;
        mi.hbrBack = popup_brush(c.popup);
        SetMenuInfo(m, &mi);
    }
    let mut n_done = 0;
    for i in 0..GetMenuItemCount(m).max(0) {
        let mut buf = [0u16; 512];
        let mut mii: MENUITEMINFOW = std::mem::zeroed();
        mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
        mii.fMask = MIIM_FTYPE | MIIM_STRING | MIIM_SUBMENU | MIIM_DATA | MIIM_BITMAP;
        mii.dwTypeData = buf.as_mut_ptr();
        mii.cch = buf.len() as u32 - 1;
        if GetMenuItemInfoW(m, i as u32, 1, &mut mii) == 0 {
            continue;
        }
        // ya es owner-draw (nuestro o de AE), lleva datos o icono de AE, o es un bitmap: no se toca,
        // salvo la línea clásica entre columnas (MFT_MENUBARBREAK → MFT_MENUBREAK)
        if mii.fType & (MFT_OWNERDRAW | MFT_BITMAP) != 0 || mii.dwItemData != 0 || !mii.hbmpItem.is_null() {
            if mii.fType & MFT_MENUBARBREAK != 0 {
                let mut set: MENUITEMINFOW = std::mem::zeroed();
                set.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
                set.fMask = MIIM_FTYPE;
                set.fType = (mii.fType & !MFT_MENUBARBREAK) | MFT_MENUBREAK;
                SetMenuItemInfoW(m, i as u32, 1, &set);
            }
            continue;
        }
        let text = &buf[..(mii.cch as usize).min(buf.len())];
        let (label, accel) = match text.iter().position(|&ch| ch == b'\t' as u16) {
            Some(p) => (text[..p].to_vec(), text[p + 1..].to_vec()),
            None => (text.to_vec(), Vec::new()),
        };
        let it = Box::into_raw(Box::new(OdItem { magic: OD_MAGIC, label, accel, sep: mii.fType & MFT_SEPARATOR != 0, sub: !mii.hSubMenu.is_null(), menu: m as isize })) as usize;
        OD_ITEMS.lock().unwrap().get_or_insert_with(HashSet::new).insert(it);
        let mut set: MENUITEMINFOW = std::mem::zeroed();
        set.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
        set.fMask = MIIM_FTYPE | MIIM_DATA;
        // la columna se separa con el fondo, sin la línea clásica en relieve
        let brk = if mii.fType & MFT_MENUBARBREAK != 0 { MFT_MENUBREAK } else { 0 };
        set.fType = (mii.fType & !MFT_MENUBARBREAK) | brk | MFT_OWNERDRAW;
        set.dwItemData = it;
        if SetMenuItemInfoW(m, i as u32, 1, &set) != 0 {
            n_done += 1;
        }
    }
    if cols {
        OD_MENUS.lock().unwrap().get_or_insert_with(HashSet::new).insert(m as isize);
    }
    if n_done > 0 {
        log(&format!("menú {:#x} (columnas: {cols}): {n_done} opciones pasan a owner-draw", m as isize));
    }
}

unsafe fn text_size(hdc: HDC, s: &[u16]) -> (i32, i32) {
    if s.is_empty() {
        return (0, 0);
    }
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    DrawTextW(hdc, s.as_ptr(), s.len() as i32, &mut r, DT_CALCRECT | DT_SINGLELINE);
    (r.right - r.left, r.bottom - r.top)
}

unsafe fn od_measure(hwnd: HWND, mis: &mut MEASUREITEMSTRUCT, it: &OdItem) {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let sc = |v: i32| v * dpi / 96;
    if it.sep {
        mis.itemWidth = sc(20) as u32;
        mis.itemHeight = sc(7) as u32;
        return;
    }
    let dc = GetDC(std::ptr::null_mut());
    let old = SelectObject(dc, menu_font(hwnd) as HGDIOBJ);
    let (lw, lh) = text_size(dc, &it.label);
    let (aw, _) = text_size(dc, &it.accel);
    let (_, fh) = text_size(dc, &wide("Ag")[..2]);
    SelectObject(dc, old);
    ReleaseDC(std::ptr::null_mut(), dc);
    let w = sc(28) + lw + if aw > 0 { sc(32) + aw } else { 0 } + sc(28);
    // Windows suma el ancho de la casilla de verificación a cada opción owner-draw
    mis.itemWidth = (w - GetSystemMetrics(SM_CXMENUCHECK)).max(sc(40)) as u32;
    mis.itemHeight = col_height(it.menu).unwrap_or(lh.max(fh) + sc(8)) as u32;
}

unsafe fn poly(hdc: HDC, pts: &[POINT], color: u32, width: i32) {
    let pen = CreatePen(PS_SOLID, width, color);
    let old = SelectObject(hdc, pen as HGDIOBJ);
    Polyline(hdc, pts.as_ptr(), pts.len() as i32);
    SelectObject(hdc, old);
    DeleteObject(pen as HGDIOBJ);
}

unsafe fn od_draw(hwnd: HWND, d: &DRAWITEMSTRUCT, it: &OdItem, c: &Colors) {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let sc = |v: i32| v * dpi / 96;
    let r = d.rcItem;
    let hdc = d.hDC;
    fill(hdc, &r, c.popup);
    if it.sep {
        let mid = (r.top + r.bottom) / 2;
        fill(hdc, &RECT { left: r.left + sc(10), top: mid, right: r.right - sc(10), bottom: mid + 1 }, c.border);
    } else {
        let sel = d.itemState & ODS_SELECTED != 0;
        let disabled = d.itemState & (ODS_GRAYED | ODS_DISABLED) != 0;
        if sel {
            fill(hdc, &RECT { left: r.left + sc(3), top: r.top + 1, right: r.right - sc(3), bottom: r.bottom - 1 }, c.popup_hover);
        }
        let col = if disabled { c.text_disabled } else if sel { c.text_hover } else { c.text };
        let old = SelectObject(hdc, menu_font(hwnd) as HGDIOBJ);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, col);
        let prefix = if d.itemState & ODS_NOACCEL != 0 { DT_HIDEPREFIX } else { 0 };
        let mut tr = RECT { left: r.left + sc(28), top: r.top, right: r.right - sc(26), bottom: r.bottom };
        if !it.label.is_empty() {
            DrawTextW(hdc, it.label.as_ptr(), it.label.len() as i32, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_LEFT | prefix);
        }
        if !it.accel.is_empty() {
            DrawTextW(hdc, it.accel.as_ptr(), it.accel.len() as i32, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_RIGHT | DT_NOPREFIX);
        }
        SelectObject(hdc, old);
        let cy = (r.top + r.bottom) / 2;
        if d.itemState & ODS_CHECKED != 0 {
            let x = r.left + sc(10);
            poly(hdc, &[POINT { x, y: cy }, POINT { x: x + sc(3), y: cy + sc(3) }, POINT { x: x + sc(9), y: cy - sc(4) }], col, sc(2).max(1));
        }
        if it.sub {
            let x = r.right - sc(13);
            poly(hdc, &[POINT { x, y: cy - sc(4) }, POINT { x: x + sc(4), y: cy }, POINT { x, y: cy + sc(4) + 1 }], col, sc(1).max(1));
        }
    }
    // que Windows no pinte encima su flecha clásica de submenú
    ExcludeClipRect(hdc, r.left, r.top, r.right, r.bottom);
}

// Ventana del menú (#32768) de un menú con columnas: el kernel la dibuja sin tema (borde clásico, huecos
// grises bajo las columnas cortas). Se subclasifica en cuanto se crea, antes de su primer dibujo, y se
// pinta también lo que Windows captura con WM_PRINT para la animación de apertura.

unsafe fn popup_menu(hwnd: HWND) -> Option<HMENU> {
    let m = SendMessageW(hwnd, MN_GETHMENU_, 0, 0) as HMENU;
    (!m.is_null() && is_od_menu(m)).then_some(m)
}

/// Origen del área cliente dentro de la ventana.
unsafe fn client_origin(hwnd: HWND) -> (i32, i32) {
    let mut wr: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut wr);
    let mut pt = POINT { x: 0, y: 0 };
    ClientToScreen(hwnd, &mut pt);
    (pt.x - wr.left, pt.y - wr.top)
}

/// Borde de la ventana (coordenadas de ventana en `hdc`).
unsafe fn paint_popup_frame(hwnd: HWND, hdc: HDC, c: &Colors) {
    let mut wr: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut wr);
    let mut cr: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut cr);
    let (ox, oy) = client_origin(hwnd);
    let full = RECT { left: 0, top: 0, right: wr.right - wr.left, bottom: wr.bottom - wr.top };
    let saved = SaveDC(hdc);
    ExcludeClipRect(hdc, ox, oy, ox + cr.right, oy + cr.bottom);
    fill(hdc, &full, c.popup);
    let b = CreateSolidBrush(c.border);
    FrameRect(hdc, &full, b);
    DeleteObject(b as HGDIOBJ);
    RestoreDC(hdc, saved);
}

/// Huecos del área cliente que no son opciones (bajo las columnas cortas, entre columnas).
/// `(ox, oy)`: dónde empieza el área cliente dentro de `hdc`.
unsafe fn paint_popup_gaps(hwnd: HWND, hdc: HDC, m: HMENU, ox: i32, oy: i32, c: &Colors) {
    let saved = SaveDC(hdc);
    let mut origin = POINT { x: 0, y: 0 };
    ClientToScreen(hwnd, &mut origin);
    for i in 0..GetMenuItemCount(m).max(0) {
        let mut ir: RECT = std::mem::zeroed();
        if GetMenuItemRect(std::ptr::null_mut(), m, i as u32, &mut ir) != 0 {
            ExcludeClipRect(hdc, ir.left - origin.x + ox, ir.top - origin.y + oy, ir.right - origin.x + ox, ir.bottom - origin.y + oy);
        }
    }
    let mut cr: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut cr);
    fill(hdc, &RECT { left: ox, top: oy, right: ox + cr.right, bottom: oy + cr.bottom }, c.popup);
    RestoreDC(hdc, saved);
}

unsafe extern "system" fn popup_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(old) = old_proc(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    let call_old = |m, w, l| CallWindowProcW(Some(std::mem::transmute::<isize, WndProcFn>(old)), hwnd, m, w, l);
    if msg == WM_NCDESTROY {
        if let Ok(mut m) = OLD_PROCS.lock() {
            if let Some(m) = m.as_mut() {
                m.remove(&(hwnd as isize));
            }
        }
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, old);
        return call_old(msg, wp, lp);
    }
    let Some(c) = colors() else { return call_old(msg, wp, lp) };
    // menú con columnas (owner-draw completo): además del marco, los huecos del área cliente
    let cols = popup_menu(hwnd);
    match msg {
        // marco exterior de todos los menús: sin el gris ni la línea clara de Windows
        WM_NCPAINT => {
            let hdc = GetWindowDC(hwnd);
            paint_popup_frame(hwnd, hdc, &c);
            ReleaseDC(hwnd, hdc);
            0
        }
        WM_ERASEBKGND if cols.is_some() => {
            let mut cr: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut cr);
            fill(wp as HDC, &cr, c.popup);
            1
        }
        WM_PAINT if cols.is_some() => {
            let r = call_old(msg, wp, lp);
            let hdc = GetDC(hwnd);
            paint_popup_gaps(hwnd, hdc, cols.unwrap(), 0, 0, &c);
            ReleaseDC(hwnd, hdc);
            r
        }
        WM_PRINTCLIENT if cols.is_some() => {
            let r = call_old(msg, wp, lp);
            paint_popup_gaps(hwnd, wp as HDC, cols.unwrap(), 0, 0, &c);
            r
        }
        WM_PRINT => {
            // la animación de apertura captura la ventana entera con WM_PRINT
            let r = call_old(msg, wp, lp);
            let hdc = wp as HDC;
            let nc = lp as u32 & PRF_NONCLIENT as u32 != 0;
            if nc {
                paint_popup_frame(hwnd, hdc, &c);
            }
            if let Some(m) = cols {
                let (ox, oy) = if nc { client_origin(hwnd) } else { (0, 0) };
                paint_popup_gaps(hwnd, hdc, m, ox, oy, &c);
            }
            r
        }
        _ => call_old(msg, wp, lp),
    }
}

/// Subclasifica la ventana de un menú (#32768) si aún no lo está.
unsafe fn hook_popup_window(hwnd: HWND) -> bool {
    let mut cls = [0u16; 16];
    let n = GetClassNameW(hwnd, cls.as_mut_ptr(), cls.len() as i32);
    if String::from_utf16_lossy(&cls[..n.max(0) as usize]) != "#32768" {
        return false;
    }
    if let Some(c) = colors() {
        // Windows 11 dibuja un borde claro alrededor de cada menú: del color de los marcos
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &c.border as *const u32 as *const c_void, 4);
        // fondo de la clase de ventana del menú: es lo que se ve un instante antes del primer dibujo
        static CLASS_BG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
        if CLASS_BG.swap(c.popup, std::sync::atomic::Ordering::Relaxed) != c.popup {
            SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, popup_brush(c.popup) as isize);
        }
    }
    let mut g = OLD_PROCS.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    if map.contains_key(&(hwnd as isize)) {
        return false;
    }
    let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, popup_proc as *const () as usize as isize);
    if old != 0 {
        map.insert(hwnd as isize, old);
    }
    true
}

const MN_GETHMENU_: u32 = 0x01E1;

unsafe extern "system" fn on_popup_start(_: HWINEVENTHOOK, _ev: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    if hwnd.is_null() || colors().is_none() {
        return;
    }
    // al mostrarse, Windows puede restablecer el borde: se vuelve a fijar
    if let Some(c) = colors() {
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &c.border as *const u32 as *const c_void, 4);
    }
    // si la ventana no se enganchó al crearse, ahora (y se repinta su marco)
    if hook_popup_window(hwnd) {
        RedrawWindow(hwnd, std::ptr::null(), std::ptr::null_mut(), RDW_FRAME | RDW_INVALIDATE);
    }
}

unsafe extern "system" fn cwp_ret(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let m = &*(lp as *const CWPRETSTRUCT);
        if m.message == WM_CREATE && colors().is_some() {
            hook_popup_window(m.hwnd);
        }
        if m.message == WM_INITMENUPOPUP {
            if let Some(c) = colors() {
                // la ventana dueña recibe WM_MEASUREITEM / WM_DRAWITEM: hay que subclasificarla
                subclass(m.hwnd);
                own_menu(m.wParam as HMENU, &c, m.hwnd);
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wp, lp)
}

/// Un hook WH_CALLWNDPROCRET por cada hilo con ventanas de AE.
unsafe fn hook_thread(hwnd: HWND) {
    hook_tid(GetWindowThreadProcessId(hwnd, std::ptr::null_mut()));
}

unsafe fn hook_tid(tid: u32) {
    let mut g = CWP_THREADS.lock().unwrap();
    let set = g.get_or_insert_with(HashSet::new);
    if tid == 0 || set.contains(&tid) {
        return;
    }
    let module = MODULE.load(std::sync::atomic::Ordering::Relaxed) as HINSTANCE;
    let h = SetWindowsHookExW(WH_CALLWNDPROCRET, Some(cwp_ret), module, tid);
    if !h.is_null() {
        log(&format!("hook de hilo {tid}"));
        set.insert(tid);
    }
}

/// Engancha ya todos los hilos de AE (los que no tienen ventanas simplemente fallan): así los menús que se
/// abran en los primeros instantes también salen pintados, sin esperar a encontrar la ventana principal.
unsafe fn hook_all_threads() {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32};
    let pid = GetCurrentProcessId();
    let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if snap == INVALID_HANDLE_VALUE {
        return;
    }
    let mut e: THREADENTRY32 = std::mem::zeroed();
    e.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut ok = Thread32First(snap, &mut e);
    while ok != 0 {
        if e.th32OwnerProcessID == pid {
            hook_tid(e.th32ThreadID);
        }
        ok = Thread32Next(snap, &mut e);
    }
    CloseHandle(snap);
}

/// Ventana de menú recién creada (aún sin mostrar): se engancha ya, para que su marco y la animación de
/// apertura salgan con nuestros colores desde el primer fotograma. Las ventanas de menú las crea el núcleo
/// de Windows y su WM_CREATE no pasa por los hooks de mensajes; este aviso sí llega.
unsafe extern "system" fn on_object_create(_: HWINEVENTHOOK, _ev: u32, hwnd: HWND, id_object: i32, _: i32, _: u32, _: u32) {
    if id_object != 0 || hwnd.is_null() || colors().is_none() {
        return; // solo ventanas (OBJID_WINDOW)
    }
    hook_popup_window(hwnd);
}

unsafe fn install_popup_hook() {
    let module = MODULE.load(std::sync::atomic::Ordering::Relaxed) as HINSTANCE;
    let pid = GetCurrentProcessId();
    let h = SetWinEventHook(EVENT_SYSTEM_MENUPOPUPSTART, EVENT_SYSTEM_MENUPOPUPSTART, module, Some(on_popup_start), pid, 0, WINEVENT_INCONTEXT);
    let h2 = SetWinEventHook(EVENT_OBJECT_CREATE, EVENT_OBJECT_CREATE, module, Some(on_object_create), pid, 0, WINEVENT_INCONTEXT);
    log(&format!("win event hooks {:?} {:?}", h, h2));
}

// ── menús desplegables (tema "Menu")

const MENU_BARBACKGROUND: i32 = 7;
const MENU_BARITEM: i32 = 8;
const MENU_POPUPBACKGROUND: i32 = 9;
const MENU_POPUPBORDERS: i32 = 10;
const MENU_POPUPGUTTER: i32 = 13;
const MENU_POPUPITEM: i32 = 14;
const MENU_POPUPSEPARATOR: i32 = 15;
// Windows 11
const MENU_POPUPITEMKBFOCUS: i32 = 26;
const MENU_POPUPITEM_FOCUSABLE: i32 = 27;

fn is_menu_part(p: i32) -> bool {
    (MENU_BARBACKGROUND..=MENU_POPUPSEPARATOR).contains(&p) || p == MENU_POPUPITEMKBFOCUS || p == MENU_POPUPITEM_FOCUSABLE
}
fn is_item_part(p: i32) -> bool {
    p == MENU_POPUPITEM || p == MENU_POPUPITEM_FOCUSABLE || p == MENU_BARITEM
}

type FnOpen = unsafe extern "system" fn(HWND, *const u16) -> isize;
type FnOpenDpi = unsafe extern "system" fn(HWND, *const u16, u32) -> isize;
type FnOpenEx = unsafe extern "system" fn(HWND, *const u16, u32) -> isize;
type FnBg = unsafe extern "system" fn(isize, HDC, i32, i32, *const RECT, *const RECT) -> i32;
type FnText = unsafe extern "system" fn(isize, HDC, i32, i32, *const u16, i32, u32, u32, *const RECT) -> i32;
type FnBgEx = unsafe extern "system" fn(isize, HDC, i32, i32, *const RECT, *const c_void) -> i32;
type FnColor = unsafe extern "system" fn(isize, i32, i32, i32, *mut u32) -> i32;
type FnTextEx = unsafe extern "system" fn(isize, HDC, i32, i32, *const u16, i32, u32, *mut RECT, *const Dttopts) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
struct Dttopts {
    size: u32,
    flags: u32,
    cr_text: u32,
    cr_border: u32,
    cr_shadow: u32,
    text_shadow_type: i32,
    shadow_offset: POINT,
    border_size: i32,
    font_prop_id: i32,
    color_prop_id: i32,
    state_id: i32,
    apply_overlay: BOOL,
    glow_size: i32,
    callback: *const c_void,
    lparam: LPARAM,
}
const DTT_TEXTCOLOR: u32 = 1;

struct Hooks {
    open: Option<RawDetour>,
    open_dpi: Option<RawDetour>,
    open_ex: Option<RawDetour>,
    bg: Option<RawDetour>,
    text: Option<RawDetour>,
    text_ex: Option<RawDetour>,
    text_ex_raw: Option<FnTextEx>,
    bg_ex: Option<RawDetour>,
    color: Option<RawDetour>,
    dwm: Option<RawDetour>,
}
unsafe impl Send for Hooks {}
unsafe impl Sync for Hooks {}

static HOOKS: OnceLock<Hooks> = OnceLock::new();
static MENU_THEMES: Mutex<Option<HashSet<isize>>> = Mutex::new(None);

unsafe fn is_menu_class(list: *const u16) -> bool {
    if list.is_null() {
        return false;
    }
    let mut n = 0;
    while *list.add(n) != 0 && n < 256 {
        n += 1;
    }
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(list, n)).to_lowercase();
    s.split(';').any(|c| c.trim() == "menu" || c.trim().ends_with("::menu"))
}

fn remember(h: isize) {
    if h != 0 {
        if let Ok(mut g) = MENU_THEMES.lock() {
            g.get_or_insert_with(HashSet::new).insert(h);
        }
    }
}

unsafe fn is_menu_theme(h: isize, hdc: HDC) -> bool {
    if MENU_THEMES.lock().ok().and_then(|g| g.as_ref().map(|s| s.contains(&h))).unwrap_or(false) {
        return true;
    }
    // tema abierto antes de cargar la DLL: se reconoce por la ventana del menú ("#32768")
    let w = WindowFromDC(hdc);
    if w.is_null() {
        return false;
    }
    let mut cls = [0u16; 16];
    let n = GetClassNameW(w, cls.as_mut_ptr(), cls.len() as i32);
    let yes = String::from_utf16_lossy(&cls[..n.max(0) as usize]) == "#32768";
    if yes {
        remember(h);
    }
    yes
}

/// ¿Este hilo está dentro de un menú abierto? Sirve para reconocer temas del menú que se abrieron antes
/// de cargar la DLL y que Windows usa con un búfer en memoria (WindowFromDC no da la ventana del menú).
unsafe fn menu_mode() -> bool {
    let mut g: GUITHREADINFO = std::mem::zeroed();
    g.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    GetGUIThreadInfo(GetCurrentThreadId(), &mut g) != 0 && g.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE) != 0
}

fn is_popup_part(p: i32) -> bool {
    matches!(p, MENU_POPUPBACKGROUND | MENU_POPUPBORDERS | MENU_POPUPGUTTER | MENU_POPUPITEM | MENU_POPUPSEPARATOR | MENU_POPUPITEMKBFOCUS | MENU_POPUPITEM_FOCUSABLE)
}

/// Llamada de dibujo que pertenece a un menú.
unsafe fn menu_call(th: isize, hdc: HDC, part: i32) -> bool {
    if is_menu_theme(th, hdc) {
        return true;
    }
    if is_popup_part(part) && menu_mode() {
        log(&format!("tema {th:#x} reconocido por modo menú (part {part})"));
        remember(th);
        return true;
    }
    false
}

unsafe extern "system" fn h_open(hwnd: HWND, list: *const u16) -> isize {
    let f: FnOpen = std::mem::transmute(HOOKS.get().unwrap().open.as_ref().unwrap().trampoline());
    let h = f(hwnd, list);
    if is_menu_class(list) {
        remember(h);
    }
    h
}
unsafe extern "system" fn h_open_dpi(hwnd: HWND, list: *const u16, dpi: u32) -> isize {
    let f: FnOpenDpi = std::mem::transmute(HOOKS.get().unwrap().open_dpi.as_ref().unwrap().trampoline());
    let h = f(hwnd, list, dpi);
    if is_menu_class(list) {
        remember(h);
    }
    h
}
unsafe extern "system" fn h_open_ex(hwnd: HWND, list: *const u16, flags: u32) -> isize {
    let f: FnOpenEx = std::mem::transmute(HOOKS.get().unwrap().open_ex.as_ref().unwrap().trampoline());
    let h = f(hwnd, list, flags);
    if is_menu_class(list) {
        remember(h);
    }
    h
}

unsafe extern "system" fn h_bg(th: isize, hdc: HDC, part: i32, state: i32, rc: *const RECT, clip: *const RECT) -> i32 {
    let orig: FnBg = std::mem::transmute(HOOKS.get().unwrap().bg.as_ref().unwrap().trampoline());
    let Some(c) = colors() else { return orig(th, hdc, part, state, rc, clip) };
    if rc.is_null() || !is_menu_part(part) || !menu_call(th, hdc, part) {
        return orig(th, hdc, part, state, rc, clip);
    }
    let r = *rc;
    log(&format!("bg part {part} state {state}"));
    match part {
        MENU_POPUPBACKGROUND | MENU_POPUPGUTTER => fill(hdc, &r, c.popup),
        MENU_POPUPBORDERS => {
            fill(hdc, &r, c.border);
            let inner = RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 };
            fill(hdc, &inner, c.popup);
        }
        MENU_POPUPITEMKBFOCUS => {
            // contorno del foco de teclado: un borde fino del color de hover
            let b = CreateSolidBrush(c.popup_hover);
            FrameRect(hdc, &r, b);
            DeleteObject(b as HGDIOBJ);
        }
        MENU_POPUPITEM | MENU_POPUPITEM_FOCUSABLE => {
            SetTextColor(hdc, text_color(&c, part, state));
            // 1 normal, 2 hot, 3 disabled, 4 disabled hot
            if state == 2 || state == 4 {
                fill(hdc, &r, c.popup_hover);
            } else {
                fill(hdc, &r, c.popup);
            }
        }
        MENU_POPUPSEPARATOR => {
            fill(hdc, &r, c.popup);
            let mid = (r.top + r.bottom) / 2;
            fill(hdc, &RECT { left: r.left, top: mid, right: r.right, bottom: mid + 1 }, c.border);
        }
        MENU_BARBACKGROUND => fill(hdc, &r, c.bar),
        MENU_BARITEM => fill(hdc, &r, if state == 2 || state == 3 { c.bar_hover } else { c.bar }),
        _ => return orig(th, hdc, part, state, rc, clip),
    }
    S_OK
}

fn text_color(c: &Colors, part: i32, state: i32) -> u32 {
    let (disabled, hot) = if part == MENU_BARITEM { (state >= 4, state == 2 || state == 3) } else { (state == 3 || state == 4, state == 2) };
    if disabled {
        c.text_disabled
    } else if hot {
        c.text_hover
    } else {
        c.text
    }
}

unsafe fn call_text_ex(th: isize, hdc: HDC, part: i32, state: i32, s: *const u16, n: i32, flags: u32, rc: *mut RECT, opts: *const Dttopts) -> i32 {
    let h = HOOKS.get().unwrap();
    let f: FnTextEx = match &h.text_ex {
        Some(d) => std::mem::transmute(d.trampoline()),
        None => h.text_ex_raw.unwrap(),
    };
    f(th, hdc, part, state, s, n, flags, rc, opts)
}

unsafe extern "system" fn h_text(th: isize, hdc: HDC, part: i32, state: i32, s: *const u16, n: i32, f1: u32, f2: u32, rc: *const RECT) -> i32 {
    let orig: FnText = std::mem::transmute(HOOKS.get().unwrap().text.as_ref().unwrap().trampoline());
    match colors() {
        Some(c) if is_item_part(part) && !rc.is_null() && menu_call(th, hdc, part) => {
            let mut o: Dttopts = std::mem::zeroed();
            o.size = std::mem::size_of::<Dttopts>() as u32;
            o.flags = DTT_TEXTCOLOR;
            o.cr_text = text_color(&c, part, state);
            let mut r = *rc;
            call_text_ex(th, hdc, part, state, s, n, f1, &mut r, &o)
        }
        _ => orig(th, hdc, part, state, s, n, f1, f2, rc),
    }
}

unsafe extern "system" fn h_text_ex(th: isize, hdc: HDC, part: i32, state: i32, s: *const u16, n: i32, flags: u32, rc: *mut RECT, opts: *const Dttopts) -> i32 {
    match colors() {
        Some(c) if is_item_part(part) && menu_call(th, hdc, part) => {
            let mut o: Dttopts = if opts.is_null() {
                let mut z: Dttopts = std::mem::zeroed();
                z.size = std::mem::size_of::<Dttopts>() as u32;
                z
            } else {
                *opts
            };
            o.flags |= DTT_TEXTCOLOR;
            o.cr_text = text_color(&c, part, state);
            call_text_ex(th, hdc, part, state, s, n, flags, rc, &o)
        }
        _ => call_text_ex(th, hdc, part, state, s, n, flags, rc, opts),
    }
}

const TMT_TEXTCOLOR: i32 = 3803;

unsafe extern "system" fn h_color(th: isize, part: i32, state: i32, prop: i32, out: *mut u32) -> i32 {
    let orig: FnColor = std::mem::transmute(HOOKS.get().unwrap().color.as_ref().unwrap().trampoline());
    if prop == TMT_TEXTCOLOR { log(&format!("color part {part} state {state}")); }
    if let Some(c) = colors() {
        if prop == TMT_TEXTCOLOR && is_item_part(part) && !out.is_null() {
            let known = MENU_THEMES.lock().ok().and_then(|g| g.as_ref().map(|s| s.contains(&th))).unwrap_or(false) || (part != MENU_BARITEM && menu_mode());
            if known {
                *out = text_color(&c, part, state);
                return S_OK;
            }
        }
    }
    orig(th, part, state, prop, out)
}

unsafe extern "system" fn h_bg_ex(th: isize, hdc: HDC, part: i32, state: i32, rc: *const RECT, opts: *const c_void) -> i32 {
    let orig: FnBgEx = std::mem::transmute(HOOKS.get().unwrap().bg_ex.as_ref().unwrap().trampoline());
    if colors().is_some() && !rc.is_null() && is_menu_part(part) && menu_call(th, hdc, part) {
        return h_bg(th, hdc, part, state, rc, std::ptr::null());
    }
    orig(th, hdc, part, state, rc, opts)
}

type FnDwmSet = unsafe extern "system" fn(HWND, u32, *const c_void, u32) -> i32;

/// AE fija él mismo el modo oscuro y los colores de la barra de título; aquí siempre ganan los nuestros.
unsafe extern "system" fn h_dwm_set(hwnd: HWND, attr: u32, pv: *const c_void, cb: u32) -> i32 {
    let orig: FnDwmSet = std::mem::transmute(HOOKS.get().unwrap().dwm.as_ref().unwrap().trampoline());
    if let Some(c) = colors() {
        let ours = match attr {
            DWMWA_CAPTION_COLOR | DWMWA_BORDER_COLOR => Some(c.title.unwrap_or(c.bar)),
            DWMWA_TEXT_COLOR => Some(c.title_text.unwrap_or(c.text)),
            _ => None,
        };
        if let Some(v) = ours {
            if cb == 4 && GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION {
                return orig(hwnd, attr, &v as *const u32 as *const c_void, 4);
            }
        }
        let r = orig(hwnd, attr, pv, cb);
        // al cambiar el modo oscuro Windows vuelve a sus colores: se reponen los nuestros
        if (attr == 19 || attr == 20) && GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION {
            for (a, v) in [(DWMWA_CAPTION_COLOR, c.title.unwrap_or(c.bar)), (DWMWA_TEXT_COLOR, c.title_text.unwrap_or(c.text)), (DWMWA_BORDER_COLOR, c.title.unwrap_or(c.bar))] {
                orig(hwnd, a, &v as *const u32 as *const c_void, 4);
            }
        }
        return r;
    }
    orig(hwnd, attr, pv, cb)
}

unsafe fn dwm_hook() -> Option<RawDetour> {
    let name = wide("dwmapi.dll");
    let mut m = GetModuleHandleW(name.as_ptr());
    if m.is_null() {
        m = LoadLibraryW(name.as_ptr());
    }
    let f = GetProcAddress(m, c"DwmSetWindowAttribute".as_ptr() as *const u8)?;
    RawDetour::new(f as *const (), h_dwm_set as *const ()).ok()
}

unsafe fn install_hooks() {
    let name = wide("uxtheme.dll");
    let mut m = GetModuleHandleW(name.as_ptr());
    if m.is_null() {
        m = LoadLibraryW(name.as_ptr());
    }
    if m.is_null() {
        return;
    }
    let addr = |n: &str| -> Option<*const ()> {
        let c = std::ffi::CString::new(n).ok()?;
        GetProcAddress(m, c.as_ptr() as *const u8).map(|f| f as *const ())
    };
    let det = |n: &str, to: *const ()| -> Option<RawDetour> {
        let d = RawDetour::new(addr(n)?, to).ok()?;
        Some(d)
    };
    let hooks = Hooks {
        open: det("OpenThemeData", h_open as *const ()),
        open_dpi: det("OpenThemeDataForDpi", h_open_dpi as *const ()),
        open_ex: det("OpenThemeDataEx", h_open_ex as *const ()),
        bg: det("DrawThemeBackground", h_bg as *const ()),
        text: det("DrawThemeText", h_text as *const ()),
        text_ex: det("DrawThemeTextEx", h_text_ex as *const ()),
        text_ex_raw: addr("DrawThemeTextEx").map(|p| std::mem::transmute::<*const (), FnTextEx>(p)),
        bg_ex: det("DrawThemeBackgroundEx", h_bg_ex as *const ()),
        color: det("GetThemeColor", h_color as *const ()),
        dwm: dwm_hook(),
    };
    if HOOKS.set(hooks).is_err() {
        return;
    }
    let h = HOOKS.get().unwrap();
    for d in [&h.open, &h.open_dpi, &h.open_ex, &h.bg, &h.text, &h.text_ex, &h.bg_ex, &h.color, &h.dwm].into_iter().flatten() {
        let _ = d.enable();
    }
}

// ── barra de título

const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_CAPTION_COLOR: u32 = 35;
const DWMWA_TEXT_COLOR: u32 = 36;
const DWMWA_COLOR_DEFAULT: u32 = 0xFFFF_FFFF;

unsafe fn paint_title(hwnd: HWND, c: Option<&Colors>) {
    let (cap, txt, brd) = match c {
        Some(c) => (c.title.unwrap_or(c.bar), c.title_text.unwrap_or(c.text), c.title.unwrap_or(c.bar)),
        None => (DWMWA_COLOR_DEFAULT, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_DEFAULT),
    };
    for (a, v) in [(DWMWA_CAPTION_COLOR, cap), (DWMWA_TEXT_COLOR, txt), (DWMWA_BORDER_COLOR, brd)] {
        let hr = DwmSetWindowAttribute(hwnd, a, &v as *const u32 as *const c_void, 4);
        log(&format!("dwm attr {a} = {v:#x} -> {hr:#x}"));
    }
}

// ── bucle: ventanas nuevas y cambios de colores

unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
    let v = &mut *(lp as *mut Vec<isize>);
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 {
        v.push(hwnd as isize);
    }
    1
}

fn load_config() -> Option<(Colors, Option<Colors>)> {
    let b = std::fs::read(config_path()).ok()?;
    let c: Config = serde_json::from_slice(&b).ok()?;
    let dark = Colors::from(&c)?;
    let light = c.light.as_deref().and_then(Colors::from).map(|mut l| {
        l.enabled = dark.enabled;
        l
    });
    Some((dark, light))
}

fn log(msg: &str) {
    static ON: OnceLock<bool> = OnceLock::new();
    // se activa con la variable AETINT_MENU_LOG o con el archivo %LOCALAPPDATA%\AeTint\debug.flag
    if !*ON.get_or_init(|| std::env::var_os("AETINT_MENU_LOG").is_some() || config_path().with_file_name("debug.flag").is_file()) {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(config_path().with_file_name("menu.log")) {
        let _ = writeln!(f, "{msg}");
    }
}

fn worker() {
    if WORKER_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    log(&format!("worker: inicia (pid {})", unsafe { GetCurrentProcessId() }));
    unsafe {
        install_hooks();
        log("worker: enganches de tema instalados");
        install_popup_hook();
    }
    log(&format!("hooks: {}", HOOKS.get().map(|h| format!("open={} dpi={} ex={} bg={} text={} textex={}", h.open.is_some(), h.open_dpi.is_some(), h.open_ex.is_some(), h.bg.is_some(), h.text.is_some(), h.text_ex.is_some())).unwrap_or_default()));
    let mut last_mtime: Option<SystemTime> = None;
    let mut seen: HashSet<isize> = HashSet::new();
    let mut seen_title: HashSet<isize> = HashSet::new();
    let mut tick: u32 = 0;
    let prefs = ae_prefs_path();
    log(&format!("preferencias de AE: {:?}", prefs));
    let mut prefs_mtime: Option<SystemTime> = None;
    loop {
        // brillo de AE (Light → colores claros); se relee si AE guarda sus preferencias
        let mut brightness_changed = false;
        if let Some(p) = &prefs {
            let m = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            if m != prefs_mtime {
                prefs_mtime = m;
                if let Some(b) = ae_brightness(p) {
                    brightness_changed = set_light_mode(b > 0.5);
                    if brightness_changed {
                        seen_title.clear();
                    }
                }
            }
        }
        let mtime = std::fs::metadata(config_path()).and_then(|m| m.modified()).ok();
        let changed = mtime != last_mtime;
        if changed {
            last_mtime = mtime;
            if let Ok(mut g) = COLORS.write() {
                *g = load_config();
            }
            seen.clear();
        }
        let c = colors();
        tick = tick.wrapping_add(1);
        if tick % 8 == 0 {
            seen_title.clear();
        }
        let mut wins: Vec<isize> = Vec::new();
        unsafe {
            EnumWindows(Some(enum_cb), &mut wins as *mut _ as LPARAM);
            for &w in &wins {
                let hwnd = w as HWND;
                if brightness_changed && !GetMenu(hwnd).is_null() {
                    DrawMenuBar(hwnd);
                    RedrawWindow(hwnd, std::ptr::null(), std::ptr::null_mut(), RDW_FRAME | RDW_INVALIDATE);
                }
                if !seen_title.contains(&w) {
                    seen_title.insert(w);
                    if GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION {
                        paint_title(hwnd, c.as_ref());
                    }
                }
                if seen.contains(&w) {
                    continue;
                }
                seen.insert(w);
                let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
                log(&format!("win {w:#x} style {style:#x} menu {:?} colors {}", GetMenu(hwnd), c.is_some()));
                if style & WS_CAPTION == WS_CAPTION {
                    paint_title(hwnd, c.as_ref());
                }
                hook_thread(hwnd);
                let menu = GetMenu(hwnd);
                if !menu.is_null() {
                    subclass(hwnd);
                    if let Some(c) = &c {
                        let br = CreateSolidBrush(c.popup);
                        let mut mi: MENUINFO = std::mem::zeroed();
                        mi.cbSize = std::mem::size_of::<MENUINFO>() as u32;
                        mi.fMask = MIM_BACKGROUND | MIM_APPLYTOSUBMENUS;
                        mi.hbrBack = br;
                        SetMenuInfo(menu, &mi);
                    }
                    DrawMenuBar(hwnd);
                }
                RedrawWindow(hwnd, std::ptr::null(), std::ptr::null_mut(), RDW_FRAME | RDW_INVALIDATE | RDW_ALLCHILDREN);
            }
        }
        seen.retain(|w| wins.contains(w));
        // los primeros 20 s (arranque de AE) más a menudo; luego basta con menos
        if tick < 80 {
            unsafe { hook_all_threads() };
            std::thread::sleep(Duration::from_millis(250));
        } else {
            std::thread::sleep(Duration::from_millis(700));
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        DisableThreadLibraryCalls(module);
        MODULE.store(module as isize, std::sync::atomic::Ordering::Relaxed);
        // con la inyección este hilo arranca bien; cargada muy pronto (desde AeTint.aex) puede no llegar a
        // correr, y por eso el plugin llama además a aetint_start
        std::thread::spawn(worker);
    }
    1
}

/// Arranca el trabajo de la DLL desde fuera de DllMain (lo llama el plugin AeTint.aex tras cargarla).
#[no_mangle]
pub extern "C" fn aetint_start() {
    log("aetint_start: llamada por el plugin");
    std::thread::spawn(worker);
}

/// Solo un hilo de trabajo, aunque se lance desde DllMain y desde aetint_start.
static WORKER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
