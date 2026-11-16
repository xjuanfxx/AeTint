//! Ventana con un menú Win32 como el de After Effects (barra + desplegables, modo oscuro del sistema).
//! Argumentos: [--dll ruta] [--open] [--menu N] [--late] [--x N]
//!   --open    abre el menú N (0 = File) a los 2,5 s
//!   --late    abre y cierra el menú una vez ANTES de cargar la DLL (como cuando se inyecta en AE ya abierto)
#![windows_subsystem = "windows"]

use std::ptr::null;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct Opts {
    dll: Option<String>,
    menu: i32,
}
static OPTS: OnceLock<Opts> = OnceLock::new();

unsafe fn open_menu(h: HWND) {
    SetForegroundWindow(h);
    let bar = GetMenu(h);
    let i = OPTS.get().map(|o| o.menu).unwrap_or(0);
    let mut r: RECT = std::mem::zeroed();
    GetMenuItemRect(h, bar, i as u32, &mut r);
    TrackPopupMenuEx(GetSubMenu(bar, i), TPM_LEFTALIGN | TPM_TOPALIGN, r.left, r.bottom, h, std::ptr::null());
}

unsafe extern "system" fn proc_(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_TIMER => {
            KillTimer(h, wp);
            match wp {
                1 | 10 => open_menu(h),
                11 => {
                    EndMenu();
                }
                12 => {
                    if let Some(d) = OPTS.get().and_then(|o| o.dll.clone()) {
                        LoadLibraryW(w(&d).as_ptr());
                    }
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(h, &mut ps);
            let b = CreateSolidBrush(0x00262626);
            FillRect(dc, &ps.rcPaint, b);
            DeleteObject(b as HGDIOBJ);
            EndPaint(h, &ps);
            0
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

fn main() {
    unsafe {
        // modo oscuro de menús como el resto de apps (uxtheme ordinal 135: SetPreferredAppMode)
        let ux = LoadLibraryW(w("uxtheme.dll").as_ptr());
        if let Some(f) = GetProcAddress(ux, 135 as *const u8) {
            let f: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            f(2);
        }
        let inst = GetModuleHandleW(null());
        let cls = w("AeTintMenuTest");
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: inst, lpszClassName: cls.as_ptr(), hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW), ..std::mem::zeroed() };
        RegisterClassW(&wc);

        let bar = CreateMenu();
        let add_popup = |title: &str, items: &[(&str, u32)]| {
            let p = CreatePopupMenu();
            for (i, (t, flags)) in items.iter().enumerate() {
                if *t == "-" {
                    AppendMenuW(p, MF_SEPARATOR, 0, null());
                } else {
                    AppendMenuW(p, MF_STRING | flags, 100 + i, w(t).as_ptr());
                }
            }
            AppendMenuW(bar, MF_POPUP, p as usize, w(title).as_ptr());
            p
        };
        let file = add_popup("&File", &[("&New\tCtrl+N", 0), ("&Open Project...\tCtrl+O", 0), ("-", 0), ("Save &As", MF_GRAYED), ("-", 0), ("E&xit", 0)]);
        let sub = CreatePopupMenu();
        AppendMenuW(sub, MF_STRING, 300, w("Composition...").as_ptr());
        AppendMenuW(sub, MF_STRING | MF_CHECKED, 301, w("Solid...").as_ptr());
        AppendMenuW(file, MF_POPUP, sub as usize, w("Import").as_ptr());
        add_popup("&Edit", &[("&Undo\tCtrl+Z", 0), ("&Redo", MF_GRAYED), ("-", 0), ("Cu&t", 0), ("&Copy", 0)]);
        for t in ["&Composition", "&Layer"] {
            add_popup(t, &[("Item 1", 0), ("Item 2", 0)]);
        }
        // Effect: muchas categorías en columnas (MF_MENUBARBREAK), como el de After Effects
        let fx = CreatePopupMenu();
        AppendMenuW(fx, MF_STRING, 500, w("Effect Controls\tF3").as_ptr());
        AppendMenuW(fx, MF_STRING, 501, w("Last Effect\tCtrl+Alt+Shift+E").as_ptr());
        AppendMenuW(fx, MF_STRING | MF_GRAYED, 502, w("Manage Effects...").as_ptr());
        AppendMenuW(fx, MF_SEPARATOR, 0, null());
        for i in 0..48 {
            let cat = CreatePopupMenu();
            AppendMenuW(cat, MF_STRING, 1000 + i * 10, w("Effect A").as_ptr());
            AppendMenuW(cat, MF_STRING, 1001 + i * 10, w("Effect B").as_ptr());
            let brk = if i == 44 { MF_MENUBARBREAK } else { 0 };
            AppendMenuW(fx, MF_POPUP | brk, cat as usize, w(&format!("Category {}", i + 1)).as_ptr());
        }
        AppendMenuW(bar, MF_POPUP, fx as usize, w("Effec&t").as_ptr());
        for t in ["&Animation", "&View", "&Window", "&Help"] {
            add_popup(t, &[("Item 1", 0), ("Item 2", 0)]);
        }

        let args: Vec<String> = std::env::args().collect();
        let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1).cloned());
        let flag = |k: &str| args.iter().any(|a| a == k);
        let x: i32 = arg("--x").and_then(|v| v.parse().ok()).unwrap_or(200);
        let late = flag("--late");
        let _ = OPTS.set(Opts { dll: arg("--dll"), menu: arg("--menu").and_then(|v| v.parse().ok()).unwrap_or(0) });
        let hwnd = CreateWindowExW(0, cls.as_ptr(), w("AeTint · prueba de menús").as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, x, 150, 900, 760, std::ptr::null_mut(), bar, inst, null());
        if late {
            SetTimer(hwnd, 10, 800, None); // abrir sin DLL
            SetTimer(hwnd, 11, 1800, None); // cerrar
            SetTimer(hwnd, 12, 2300, None); // cargar la DLL
            SetTimer(hwnd, 1, 4500, None); // abrir otra vez
        } else {
            // la DLL se carga aquí mismo (sin inyección)
            if let Some(dll) = arg("--dll") {
                LoadLibraryW(w(&dll).as_ptr());
            }
            if flag("--open") {
                SetTimer(hwnd, 1, 2500, None);
            }
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
