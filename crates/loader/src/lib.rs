//! Plugin de After Effects (`AeTint.aex`, en la carpeta Plug-ins). After Effects lo carga solo al arrancar,
//! antes de mostrar su ventana, y lo único que hace es cargar `aetint_menu.dll`. Así los menús y la barra
//! de título salen pintados desde el principio, sin inyección ni proceso vigilante, y las mejoras llegan
//! actualizando solo la DLL.
//!
//! Dónde está la DLL: AeTint escribe su ruta en `%LOCALAPPDATA%\AeTint\dll_path.txt` (la carpeta de AeTint;
//! desde `AppData` After Effects no la deja cargar). Lo que pasa queda en `%LOCALAPPDATA%\AeTint\loader.log`.
//!
//! La carga se hace desde un hilo propio del plugin, que arranca al abrirse el .aex (DllMain) y reintenta
//! unos segundos, y también desde la función de entrada si AE la llama. Mientras AE inicializa los plugins
//! la función de entrada no llega a ver el archivo; el hilo sí.

use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{GetLastError, BOOL, HINSTANCE};
use windows_sys::Win32::System::LibraryLoader::{DisableThreadLibraryCalls, GetModuleHandleW, GetProcAddress, LoadLibraryW};

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|b| PathBuf::from(b).join("AeTint"))
}

fn log(msg: &str) {
    let Some(d) = data_dir() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join("loader.log")) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{t}] pid {} {msg}", std::process::id());
    }
}

/// Ruta de la DLL: la que dejó anotada AeTint; si no, la copia de %LOCALAPPDATA%\AeTint.
fn dll_path() -> Option<PathBuf> {
    let d = data_dir()?;
    if let Ok(s) = std::fs::read_to_string(d.join("dll_path.txt")) {
        let p = PathBuf::from(s.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    let p = d.join("aetint_menu.dll");
    p.is_file().then_some(p)
}

fn loaded() -> bool {
    unsafe { !GetModuleHandleW(wide(std::ffi::OsStr::new("aetint_menu.dll")).as_ptr()).is_null() }
}

/// Arranca el trabajo de la DLL (su hilo de DllMain puede no correr si se carga tan pronto).
fn start_menu_dll(from: &str) {
    unsafe {
        let h = GetModuleHandleW(wide(std::ffi::OsStr::new("aetint_menu.dll")).as_ptr());
        if h.is_null() {
            return;
        }
        match GetProcAddress(h, c"aetint_start".as_ptr() as *const u8) {
            Some(f) => {
                let f: extern "C" fn() = std::mem::transmute(f);
                f();
                log(&format!("{from}: aetint_start llamada"));
            }
            None => log(&format!("{from}: la DLL no tiene aetint_start")),
        }
    }
}

/// Un intento de cargar la DLL. Devuelve true si queda cargada.
fn load_menu_dll(from: &str) -> bool {
    if loaded() {
        return true;
    }
    let Some(dll) = dll_path() else {
        log(&format!("{from}: no se encontró aetint_menu.dll"));
        return false;
    };
    let h = unsafe { LoadLibraryW(wide(dll.as_os_str()).as_ptr()) };
    if h.is_null() {
        log(&format!("{from}: no se pudo cargar {} (error {})", dll.display(), unsafe { GetLastError() }));
        false
    } else {
        log(&format!("{from}: cargada {}", dll.display()));
        true
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut core::ffi::c_void) -> BOOL {
    if reason == 1 {
        // DLL_PROCESS_ATTACH (dentro de DllMain no se puede cargar otra DLL: se hace en un hilo)
        DisableThreadLibraryCalls(module);
        log("DllMain: AE abrió el plugin");
        // hilo propio del plugin: reintenta unos segundos (mientras AE inicializa los plugins puede no ver el archivo)
        std::thread::spawn(|| {
            for i in 0..20 {
                if load_menu_dll(if i == 0 { "hilo" } else { "hilo (reintento)" }) {
                    start_menu_dll("hilo");
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(750));
            }
        });
    }
    1
}

/// Función de entrada de un plugin general (AEGP):
/// `A_Err EntryPointFunc(SPBasicSuite*, A_long major, A_long minor, AEGP_PluginID, AEGP_GlobalRefcon*)`.
/// Devuelve 0 (sin error) pase lo que pase.
#[no_mangle]
pub unsafe extern "C" fn EntryPointFunc(_basic: *mut core::ffi::c_void, _major: i32, _minor: i32, _id: i32, _refcon: *mut *mut core::ffi::c_void) -> i32 {
    log("EntryPointFunc llamada por After Effects");
    if load_menu_dll("EntryPointFunc") {
        start_menu_dll("EntryPointFunc");
    }
    0
}
