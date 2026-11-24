//! Menús y barra de título de After Effects: colores para aetint_menu.dll, carga de la DLL en AfterFX.exe
//! (sin tocar archivos de Adobe) y el vigilante que lo hace cada vez que se abre After Effects.

use crate::color;
use crate::install::data_dir;
use crate::remap::Palette;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;

pub const DLL_NAME: &str = "aetint_menu.dll";
const TARGET_EXE: &str = "afterfx.exe";

/// Un juego de colores de menú (oscuro o claro).
#[derive(Serialize)]
struct MenuSet {
    bar: String,
    bar_hover: String,
    text: String,
    text_hover: String,
    text_disabled: String,
    popup: String,
    popup_hover: String,
    border: String,
    title: String,
    title_text: String,
}

/// menu.json: el juego oscuro va en la raíz (como siempre) y el claro en "light".
#[derive(Serialize)]
struct MenuColors {
    enabled: bool,
    #[serde(flatten)]
    dark: MenuSet,
    light: MenuSet,
}

fn menu_set(p: &Palette) -> Result<MenuSet, String> {
    let pal = p.resolve()?;
    let h = color::to_hex;
    Ok(MenuSet {
        // barra de menús y de título: entre los paneles y el fondo (un toque más oscura que los paneles para
        // distinguirse, sin llegar al negro del fondo); la vista previa usa el mismo tono
        bar: h(color::mix(pal.panel, pal.base, 0.5)),
        bar_hover: h(color::mix(color::mix(pal.panel, pal.base, 0.5), pal.accent, 0.25)),
        text: h(pal.text),
        text_hover: h(color::mix(pal.text, [1.0; 3], 0.35)),
        text_disabled: h(color::mix(pal.text, pal.panel, 0.55)),
        popup: h(pal.panel),
        popup_hover: h(color::mix(pal.frame, pal.accent, 0.25)),
        border: h(pal.frame),
        title: h(color::mix(pal.panel, pal.base, 0.5)),
        title_text: h(pal.text),
    })
}

/// Escribe %LOCALAPPDATA%\AeTint\menu.json con los colores de los brillos oscuros y del claro.
/// La DLL lo relee sola (AE cambia de color en vivo) y elige el juego según el brillo de AE.
pub fn write_colors(dark: &Palette, light: &Palette, enabled: bool) -> Result<(), String> {
    let m = MenuColors { enabled, dark: menu_set(dark)?, light: menu_set(light)? };
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    std::fs::write(data_dir().join("menu.json"), serde_json::to_vec_pretty(&m).unwrap()).map_err(|e| e.to_string())
}

/// La DLL que viene junto a AeTint.exe. El vigilante corre desde una copia en %LOCALAPPDATA%\AeTint: allí la
/// toma de la ruta anotada en dll_path.txt.
fn bundled_dll() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.with_file_name(DLL_NAME);
    if p.is_file() {
        return Some(p);
    }
    let p = PathBuf::from(std::fs::read_to_string(data_dir().join("dll_path.txt")).ok()?.trim());
    p.is_file().then_some(p)
}

/// Nombre del proceso del vigilante: una copia de AeTint con otro nombre, para que no se confunda con la app.
pub const WATCHDOG_EXE: &str = "AeTintWatchdog.exe";

/// La copia del vigilante, al día con este AeTint.
fn watchdog_exe() -> Result<PathBuf, String> {
    let src = std::env::current_exe().map_err(|e| e.to_string())?;
    let dst = data_dir().join(WATCHDOG_EXE);
    if src == dst {
        return Ok(dst);
    }
    let same = match (std::fs::metadata(&src), std::fs::metadata(&dst)) {
        (Ok(a), Ok(b)) => a.len() == b.len() && a.modified().ok() <= b.modified().ok(),
        _ => false,
    };
    if !same {
        std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::copy(&src, &dst) {
            // en uso por un vigilante que ya corre: vale la copia que hay
            if !dst.is_file() {
                return Err(e.to_string());
            }
        }
    }
    Ok(dst)
}

/// Cierra el vigilante si está corriendo: el de su ventana de la bandeja (también el de versiones anteriores, que
/// corría como aetint.exe --watch).
pub fn stop_watcher() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_TERMINATE, PROCESS_SYNCHRONIZE};
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
        let cls: Vec<u16> = "AeTintWatcherTray".encode_utf16().chain(Some(0)).collect();
        for _ in 0..4 {
            let h = FindWindowW(cls.as_ptr(), std::ptr::null());
            if h.is_null() {
                break;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid == 0 || pid == std::process::id() {
                break;
            }
            let p = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
            if p.is_null() {
                break;
            }
            TerminateProcess(p, 0);
            WaitForSingleObject(p, 2000);
            CloseHandle(p);
        }
    }
}

/// Deja anotada la ruta de la DLL (junto a AeTint.exe) en %LOCALAPPDATA%\AeTint\dll_path.txt: de ahí la lee
/// el plugin AeTint.aex. La DLL se carga desde la carpeta de AeTint porque desde AppData After Effects no la
/// deja cargar.
pub fn deploy_dll() {
    let Some(src) = bundled_dll() else { return };
    let _ = std::fs::create_dir_all(data_dir());
    let f = data_dir().join("dll_path.txt");
    let s = src.display().to_string();
    if std::fs::read_to_string(&f).map(|x| x.trim() != s).unwrap_or(true) {
        let _ = std::fs::write(&f, s);
    }
    // la copia antigua en AppData ya no se usa
    let _ = std::fs::remove_file(data_dir().join(DLL_NAME));
}

pub fn dll_path() -> Option<PathBuf> {
    deploy_dll();
    bundled_dll()
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
    use windows_sys::Win32::System::Threading::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn name(buf: &[u16]) -> String {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }

    pub fn ae_pids() -> Vec<u32> {
        let mut out = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e);
            while ok != 0 {
                if name(&e.szExeFile).to_lowercase() == TARGET_EXE {
                    out.push(e.th32ProcessID);
                }
                ok = Process32NextW(snap, &mut e);
            }
            CloseHandle(snap);
        }
        out
    }

    pub fn has_module(pid: u32, module: &str) -> bool {
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
            if snap == INVALID_HANDLE_VALUE {
                return false;
            }
            let mut e: MODULEENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
            let mut ok = Module32FirstW(snap, &mut e);
            let mut found = false;
            while ok != 0 {
                if name(&e.szModule).eq_ignore_ascii_case(module) {
                    found = true;
                    break;
                }
                ok = Module32NextW(snap, &mut e);
            }
            CloseHandle(snap);
            found
        }
    }

    /// Carga la DLL dentro del proceso (LoadLibraryW en un hilo remoto). Mismo usuario: no hace falta admin.
    pub fn inject(pid: u32, dll: &std::path::Path) -> Result<(), String> {
        let path: Vec<u16> = dll.as_os_str().encode_wide().chain(Some(0)).collect();
        let bytes = path.len() * 2;
        unsafe {
            let h = OpenProcess(PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ, 0, pid);
            if h.is_null() {
                return Err(tr!("couldn't open process {} (error {})", "no se pudo abrir el proceso {} (error {})", pid, GetLastError()));
            }
            let mem = VirtualAllocEx(h, std::ptr::null(), bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
            if mem.is_null() {
                CloseHandle(h);
                return Err(tr!("VirtualAllocEx failed", "VirtualAllocEx falló"));
            }
            let mut written = 0;
            if WriteProcessMemory(h, mem, path.as_ptr() as *const _, bytes, &mut written) == 0 {
                VirtualFreeEx(h, mem, 0, MEM_RELEASE);
                CloseHandle(h);
                return Err(tr!("WriteProcessMemory failed", "WriteProcessMemory falló"));
            }
            let k32 = GetModuleHandleW(wide("kernel32.dll").as_ptr());
            let Some(ll) = GetProcAddress(k32, c"LoadLibraryW".as_ptr() as *const u8) else {
                CloseHandle(h);
                return Err(tr!("LoadLibraryW not found", "LoadLibraryW no encontrada"));
            };
            let start: LPTHREAD_START_ROUTINE = Some(std::mem::transmute::<_, unsafe extern "system" fn(*mut core::ffi::c_void) -> u32>(ll));
            let th = CreateRemoteThread(h, std::ptr::null(), 0, start, mem, 0, std::ptr::null_mut());
            if th.is_null() {
                VirtualFreeEx(h, mem, 0, MEM_RELEASE);
                CloseHandle(h);
                return Err(tr!("CreateRemoteThread failed (error {})", "CreateRemoteThread falló (error {})", GetLastError()));
            }
            WaitForSingleObject(th, 10_000);
            CloseHandle(th);
            VirtualFreeEx(h, mem, 0, MEM_RELEASE);
            CloseHandle(h);
        }
        Ok(())
    }

    /// Un solo vigilante a la vez (mutex con nombre). Devuelve false si ya hay otro.
    pub fn single_instance() -> bool {
        unsafe {
            let m = CreateMutexW(std::ptr::null(), 1, wide("Local\\AeTint.MenuWatcher").as_ptr());
            !m.is_null() && GetLastError() != ERROR_ALREADY_EXISTS
        }
    }

    pub static WATCH_DLL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    static SEEN: std::sync::Mutex<Option<HashSet<u32>>> = std::sync::Mutex::new(None);
    static NOT_AE: std::sync::Mutex<Option<HashSet<u32>>> = std::sync::Mutex::new(None);

    fn exe_name(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut n = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0;
            CloseHandle(h);
            ok.then(|| String::from_utf16_lossy(&buf[..n as usize]).rsplit('\\').next().unwrap_or("").to_lowercase())
        }
    }

    /// Un After Effects nuevo: espera a que arranque, le carga la DLL si hace falta y lo olvida al cerrarse.
    pub fn on_ae_process(pid: u32) {
        if !SEEN.lock().unwrap().get_or_insert_with(HashSet::new).insert(pid) {
            return;
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(4));
            if super::enabled() && !has_module(pid, DLL_NAME) {
                if let Some(dll) = WATCH_DLL.get() {
                    let _ = inject(pid, dll);
                }
            }
            unsafe {
                let h = OpenProcess(SYNCHRONIZATION_SYNCHRONIZE, 0, pid);
                if !h.is_null() {
                    WaitForSingleObject(h, INFINITE);
                    CloseHandle(h);
                }
            }
            if let Some(s) = SEEN.lock().unwrap().as_mut() {
                s.remove(&pid);
            }
        });
    }

    unsafe extern "system" fn on_show(_: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK, _: u32, hwnd: windows_sys::Win32::Foundation::HWND, id_object: i32, _: i32, _: u32, _: u32) {
        if id_object != 0 || hwnd.is_null() {
            return; // solo ventanas (OBJID_WINDOW)
        }
        let mut pid = 0;
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 || SEEN.lock().unwrap().as_ref().is_some_and(|s| s.contains(&pid)) {
            return;
        }
        {
            let mut g = NOT_AE.lock().unwrap();
            let set = g.get_or_insert_with(HashSet::new);
            if set.contains(&pid) {
                return;
            }
            if exe_name(pid).as_deref() != Some(TARGET_EXE) {
                if set.len() > 4096 {
                    set.clear();
                }
                set.insert(pid);
                return;
            }
        }
        on_ae_process(pid);
    }

    /// Duerme hasta que Windows avise de una ventana nueva (EVENT_OBJECT_SHOW), sin gastar CPU mientras tanto.
    pub fn wait_for_windows() {
        use windows_sys::Win32::UI::Accessibility::SetWinEventHook;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, std::ptr::null_mut(), Some(on_show), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
            tray::create();
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            tray::remove();
        }
    }

    /// Icono en la bandeja del sistema mientras el vigilante está en marcha, para que se sepa qué es ese proceso.
    /// Doble clic: abrir AeTint. Clic derecho: abrir AeTint o detener el vigilante.
    mod tray {
        use super::wide;
        use crate::i18n::t;
        use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
        use windows_sys::Win32::Foundation::*;
        use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
        use windows_sys::Win32::UI::Shell::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        const WM_TRAY: u32 = WM_APP + 1;
        const ID_OPEN: usize = 1;
        const ID_STOP: usize = 2;
        static HWND_TRAY: AtomicIsize = AtomicIsize::new(0);
        static WM_TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

        unsafe fn data(hwnd: HWND) -> NOTIFYICONDATAW {
            let mut d: NOTIFYICONDATAW = std::mem::zeroed();
            d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            d.hWnd = hwnd;
            d.uID = 1;
            d
        }

        unsafe fn add(hwnd: HWND) {
            let mut d = data(hwnd);
            d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            d.uCallbackMessage = WM_TRAY;
            let exe: Vec<u16> = std::env::current_exe().map(|p| wide(&p.display().to_string())).unwrap_or_default();
            d.hIcon = ExtractIconW(GetModuleHandleW(std::ptr::null()), exe.as_ptr(), 0);
            let tip = wide(t("AeTint · paints the After Effects menus", "AeTint · pinta los menús de After Effects"));
            for (i, c) in tip.iter().take(d.szTip.len() - 1).enumerate() {
                d.szTip[i] = *c;
            }
            Shell_NotifyIconW(NIM_ADD, &d);
        }

        fn open_aetint() {
            // el vigilante corre desde su copia: la app de verdad está junto a la DLL
            let app = super::bundled_dll().and_then(|d| d.parent().map(|p| p.join("AeTint.exe"))).filter(|p| p.is_file());
            if let Some(exe) = app.or_else(|| std::env::current_exe().ok()) {
                let _ = std::process::Command::new(exe).spawn();
            }
        }

        unsafe fn show_menu(hwnd: HWND) {
            let m = CreatePopupMenu();
            AppendMenuW(m, MF_STRING, ID_OPEN, wide(t("Open AeTint", "Abrir AeTint")).as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(m, MF_STRING, ID_STOP, wide(t("Stop (until AeTint opens again)", "Detener (hasta que se abra AeTint)")).as_ptr());
            SetMenuDefaultItem(m, ID_OPEN as u32, 0);
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, std::ptr::null()) as usize;
            DestroyMenu(m);
            match cmd {
                ID_OPEN => open_aetint(),
                ID_STOP => PostQuitMessage(0),
                _ => {}
            }
        }

        unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            if msg == WM_TRAY {
                match (lp & 0xffff) as u32 {
                    WM_LBUTTONDBLCLK => open_aetint(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                    _ => {}
                }
                return 0;
            }
            // el Explorador se reinició: volver a poner el icono
            if msg != 0 && msg == WM_TASKBAR_CREATED.load(Ordering::Relaxed) {
                add(hwnd);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }

        pub unsafe fn create() {
            // menús oscuros como el resto de Windows (uxtheme 135: SetPreferredAppMode, 136: FlushMenuThemes)
            let ux = LoadLibraryW(wide("uxtheme.dll").as_ptr());
            if !ux.is_null() {
                if let Some(f) = GetProcAddress(ux, 135 as *const u8) {
                    let f: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
                    f(1);
                }
                if let Some(f) = GetProcAddress(ux, 136 as *const u8) {
                    let f: unsafe extern "system" fn() = std::mem::transmute(f);
                    f();
                }
            }
            let inst = GetModuleHandleW(std::ptr::null());
            let cls = wide("AeTintWatcherTray");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: inst, lpszClassName: cls.as_ptr(), ..std::mem::zeroed() };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(0, cls.as_ptr(), wide("AeTint").as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null());
            if hwnd.is_null() {
                return;
            }
            WM_TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);
            HWND_TRAY.store(hwnd as isize, Ordering::Relaxed);
            add(hwnd);
        }

        pub unsafe fn remove() {
            let h = HWND_TRAY.load(Ordering::Relaxed) as HWND;
            if !h.is_null() {
                let d = data(h);
                Shell_NotifyIconW(NIM_DELETE, &d);
            }
        }
    }

    pub fn watcher_running() -> bool {
        unsafe {
            let m = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, wide("Local\\AeTint.MenuWatcher").as_ptr());
            if m.is_null() {
                return false;
            }
            CloseHandle(m);
            true
        }
    }
}

#[cfg(windows)]
pub use win::{ae_pids, inject, watcher_running};

fn enabled() -> bool {
    std::fs::read(data_dir().join("menu.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["enabled"].as_bool())
        .unwrap_or(false)
}

/// Modo `--watch`: respaldo para las versiones de AE sin el plugin AeTint.aex. No mira cada pocos segundos:
/// se queda dormido y Windows le avisa cada vez que aparece una ventana; si es de After Effects, le carga la
/// DLL (si el plugin no lo hizo ya).
#[cfg(windows)]
pub fn watch() -> i32 {
    if !win::single_instance() {
        return 0;
    }
    // idioma del icono de la bandeja
    crate::i18n::set_lang(&crate::install::load_config().lang);
    let Some(dll) = dll_path() else { return 1 };
    let _ = win::WATCH_DLL.set(dll);
    // los After Effects que ya estén abiertos
    for pid in ae_pids() {
        win::on_ae_process(pid);
    }
    win::wait_for_windows();
    0
}

/// Arranca el vigilante en segundo plano (si no está ya).
pub fn start_watcher() -> Result<(), String> {
    #[cfg(windows)]
    {
        if watcher_running() {
            return Ok(());
        }
        use std::os::windows::process::CommandExt;
        let exe = watchdog_exe()?;
        std::process::Command::new(exe)
            .arg("--watch")
            .creation_flags(0x0800_0000 | 0x0000_0008) // sin ventana, desacoplado
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Inicio con Windows (HKCU\…\Run).
pub fn set_autostart(on: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let exe = watchdog_exe()?;
        let mut c = std::process::Command::new("reg");
        if on {
            c.args(["add", key, "/v", "AeTint", "/t", "REG_SZ", "/f", "/d", &format!("\"{}\" --watch", exe.display())]);
        } else {
            c.args(["delete", key, "/v", "AeTint", "/f"]);
        }
        let st = c.creation_flags(0x0800_0000).status().map_err(|e| e.to_string())?;
        if !st.success() && on {
            return Err(tr!("couldn't set up start with Windows", "no se pudo escribir el inicio automático"));
        }
    }
    Ok(())
}

pub fn autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        return std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "AeTint"])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }
    #[allow(unreachable_code)]
    false
}
