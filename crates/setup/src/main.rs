//! AeTint-Setup.exe: instala AeTint para el usuario en %LOCALAPPDATA%\Programs\AeTint (sin permisos de
//! administrador), con accesos directos y entrada en Configuración → Aplicaciones. Con `--uninstall` hace de
//! desinstalador (es la copia que se deja dentro de la carpeta de instalación).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use slint::ComponentHandle;
use std::path::{Path, PathBuf};
use std::process::Command;

slint::include_modules!();

const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\AeTint";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const NO_WINDOW: u32 = 0x0800_0000;

/// Los archivos de AeTint que lleva dentro el instalador.
const PAYLOAD: &[(&str, &[u8])] = &[
    ("AeTint.exe", include_bytes!(concat!(env!("OUT_DIR"), "/payload/AeTint.exe"))),
    ("aetint_menu.dll", include_bytes!(concat!(env!("OUT_DIR"), "/payload/aetint_menu.dll"))),
    ("aetint_loader.dll", include_bytes!(concat!(env!("OUT_DIR"), "/payload/aetint_loader.dll"))),
];

fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}
fn install_dir() -> PathBuf {
    local_appdata().join("Programs").join("AeTint")
}
fn data_dir() -> PathBuf {
    local_appdata().join("AeTint")
}
fn start_menu_lnk() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    base.join(r"Microsoft\Windows\Start Menu\Programs\AeTint.lnk")
}
fn desktop_lnk() -> PathBuf {
    let base = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    base.join("Desktop").join("AeTint.lnk")
}

/// Idioma: el que eligió el usuario en AeTint; si no, el de Windows.
fn spanish() -> bool {
    if let Ok(s) = std::fs::read_to_string(data_dir().join("config.json")) {
        if s.contains("\"lang\": \"es\"") || s.contains("\"lang\":\"es\"") {
            return true;
        }
        if s.contains("\"lang\"") {
            return false;
        }
    }
    #[cfg(windows)]
    unsafe {
        (windows_sys::Win32::Globalization::GetUserDefaultUILanguage() & 0x3ff) == 0x0a
    }
    #[cfg(not(windows))]
    false
}

fn t(es: bool, en: &str, es_text: &str) -> String {
    if es { es_text } else { en }.to_string()
}

fn cmd(program: &str) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(NO_WINDOW);
    }
    c
}

/// Acceso directo (.lnk) mediante WScript.Shell.
fn shortcut(lnk: &Path, target: &Path) -> bool {
    let ps = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';$s.Description='AeTint';$s.Save()",
        lnk.display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''"),
        target.parent().unwrap().display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''"),
    );
    if let Some(p) = lnk.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    cmd("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &ps]).status().map(|s| s.success()).unwrap_or(false)
}

fn reg_add(key: &str, name: &str, kind: &str, value: &str) {
    let _ = cmd("reg").args(["add", key, "/v", name, "/t", kind, "/d", value, "/f"]).status();
}

fn reg_value_exists(key: &str, name: &str) -> bool {
    cmd("reg").args(["query", key, "/v", name]).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Cierra los AeTint.exe que corren desde la carpeta de instalación (la app o su vigilante).
#[cfg(windows)]
fn stop_installed_aetint(dir: &Path) {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    use windows_sys::Win32::System::Threading::*;
    let dir = dir.to_string_lossy().to_lowercase();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            if String::from_utf16_lossy(&e.szExeFile[..n]).eq_ignore_ascii_case("AeTint.exe") {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, e.th32ProcessID);
                if !h.is_null() {
                    let mut buf = [0u16; 1024];
                    let mut len = buf.len() as u32;
                    if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 && String::from_utf16_lossy(&buf[..len as usize]).to_lowercase().starts_with(&dir) {
                        TerminateProcess(h, 0);
                        WaitForSingleObject(h, 3000);
                    }
                    CloseHandle(h);
                }
            }
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
}
#[cfg(not(windows))]
fn stop_installed_aetint(_: &Path) {}

struct Opts {
    start_menu: bool,
    desktop: bool,
    restore: bool,
    wipe: bool,
}

type Report = Box<dyn Fn(f32, String) + Send>;

fn install(o: &Opts, es: bool, report: &Report) -> Result<String, String> {
    let dir = install_dir();
    report(0.05, t(es, "Closing AeTint…", "Cerrando AeTint…"));
    stop_installed_aetint(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // restos de una actualización anterior (DLL que After Effects tenía en uso)
    for (name, _) in PAYLOAD {
        let _ = std::fs::remove_file(dir.join(format!("{name}.old")));
    }
    for (i, (name, bytes)) in PAYLOAD.iter().enumerate() {
        report(0.15 + 0.15 * i as f32, format!("{} {name}…", t(es, "Copying", "Copiando")));
        let dst = dir.join(name);
        let mut r = std::fs::write(&dst, bytes);
        // en uso (After Effects tiene cargada la DLL de menús): Windows deja renombrarla; AE seguirá con la
        // vieja hasta reiniciarse y la nueva queda en su sitio
        if r.as_ref().is_err_and(|e| e.raw_os_error() == Some(32) || e.raw_os_error() == Some(5)) {
            let old = dir.join(format!("{name}.old"));
            let _ = std::fs::remove_file(&old);
            if std::fs::rename(&dst, &old).is_ok() {
                r = std::fs::write(&dst, bytes);
            }
        }
        r.map_err(|e| {
            if e.raw_os_error() == Some(32) {
                t(es, &format!("{name} is in use. Close After Effects and try again."), &format!("{name} está en uso. Cierra After Effects y vuelve a intentarlo."))
            } else {
                format!("{name}: {e}")
            }
        })?;
    }
    // este mismo programa queda como desinstalador
    report(0.62, t(es, "Registering the uninstaller…", "Registrando el desinstalador…"));
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninstaller = dir.join("AeTint-Setup.exe");
    if me != uninstaller {
        let _ = std::fs::copy(&me, &uninstaller);
    }
    let exe = dir.join("AeTint.exe");
    let size_kb: u64 = PAYLOAD.iter().map(|(_, b)| b.len() as u64).sum::<u64>() / 1024;
    reg_add(UNINSTALL_KEY, "DisplayName", "REG_SZ", "AeTint");
    reg_add(UNINSTALL_KEY, "DisplayVersion", "REG_SZ", VERSION);
    reg_add(UNINSTALL_KEY, "Publisher", "REG_SZ", "xjuanfxx");
    reg_add(UNINSTALL_KEY, "DisplayIcon", "REG_SZ", &exe.display().to_string());
    reg_add(UNINSTALL_KEY, "InstallLocation", "REG_SZ", &dir.display().to_string());
    reg_add(UNINSTALL_KEY, "URLInfoAbout", "REG_SZ", "https://github.com/xjuanfxx/AeTint");
    reg_add(UNINSTALL_KEY, "UninstallString", "REG_SZ", &format!("\"{}\" --uninstall", uninstaller.display()));
    reg_add(UNINSTALL_KEY, "NoModify", "REG_DWORD", "1");
    reg_add(UNINSTALL_KEY, "NoRepair", "REG_DWORD", "1");
    reg_add(UNINSTALL_KEY, "EstimatedSize", "REG_DWORD", &size_kb.to_string());
    // si AeTint arrancaba con Windows desde otra carpeta, ahora desde la instalada
    if reg_value_exists(RUN_KEY, "AeTint") {
        reg_add(RUN_KEY, "AeTint", "REG_SZ", &format!("\"{}\" --watch", exe.display()));
    }
    report(0.8, t(es, "Creating shortcuts…", "Creando accesos directos…"));
    if o.start_menu {
        shortcut(&start_menu_lnk(), &exe);
    }
    if o.desktop {
        shortcut(&desktop_lnk(), &exe);
    }
    report(1.0, String::new());
    Ok(t(es, &format!("Installed in {}.", dir.display()), &format!("Instalado en {}.", dir.display())))
}

fn uninstall(o: &Opts, es: bool, report: &Report) -> Result<String, String> {
    let dir = install_dir();
    let exe = dir.join("AeTint.exe");
    let mut restored = false;
    if o.restore && exe.is_file() {
        report(0.15, t(es, "Restoring After Effects…", "Restaurando After Effects…"));
        let ok = Command::new(&exe).arg("--restore-all").status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return Err(t(
                es,
                "After Effects couldn't be restored. Close After Effects (and accept the administrator prompt) and try again, or untick that option.",
                "No se pudo restaurar After Effects. Ciérralo (y acepta el permiso de administrador) y vuelve a intentarlo, o desmarca esa opción.",
            ));
        }
        restored = true;
    }
    report(0.45, t(es, "Closing AeTint…", "Cerrando AeTint…"));
    stop_installed_aetint(&dir);
    report(0.6, t(es, "Removing shortcuts…", "Quitando accesos directos…"));
    let _ = std::fs::remove_file(start_menu_lnk());
    let _ = std::fs::remove_file(desktop_lnk());
    let _ = cmd("reg").args(["delete", UNINSTALL_KEY, "/f"]).status();
    let _ = cmd("reg").args(["delete", RUN_KEY, "/v", "AeTint", "/f"]).status();
    if o.wipe {
        report(0.75, t(es, "Deleting settings…", "Borrando la configuración…"));
        let d = data_dir();
        if restored {
            let _ = std::fs::remove_dir_all(&d);
        } else if let Ok(rd) = std::fs::read_dir(&d) {
            // sin restaurar, las copias de Adobe (y el registro de lo escrito) se conservan
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n != "originals" && n != "state.json" {
                    let p = e.path();
                    let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                }
            }
        }
    }
    report(0.9, t(es, "Removing files…", "Quitando archivos…"));
    for (name, _) in PAYLOAD {
        let _ = std::fs::remove_file(dir.join(name));
        let _ = std::fs::remove_file(dir.join(format!("{name}.old")));
    }
    report(1.0, String::new());
    Ok(if restored {
        t(es, "After Effects is back to its original colours.", "After Effects vuelve a tener sus colores originales.")
    } else {
        t(es, "After Effects keeps the theme it has now.", "After Effects conserva el tema que tiene ahora.")
    })
}

/// El desinstalador no puede borrarse a sí mismo: lo hace un cmd oculto al cerrarse.
fn delete_install_dir_after_exit() {
    let dir = install_dir();
    let script = format!("/c ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"", dir.display());
    let mut c = cmd("cmd");
    // cmd no entiende las comillas escapadas que pone Rust: el comando va tal cual
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.raw_arg(&script);
    }
    let _ = c.spawn();
}

fn main() -> Result<(), slint::PlatformError> {
    let uninstall_mode = std::env::args().any(|a| a == "--uninstall");
    // lanzado por AeTint para actualizarse: instala sin preguntar y vuelve a abrir AeTint
    let update_mode = !uninstall_mode && std::env::args().any(|a| a == "--update");
    let es = spanish();
    let ui = SetupWindow::new()?;
    ui.global::<L>().set_es(es);
    ui.set_uninstall(uninstall_mode);
    ui.set_version(VERSION.into());
    ui.set_install_dir(install_dir().display().to_string().into());
    ui.set_updating(!uninstall_mode && install_dir().join("AeTint.exe").is_file());
    ui.set_payload_ok(PAYLOAD.iter().all(|(_, b)| !b.is_empty()));
    if update_mode {
        // se conservan los accesos directos que ya había
        ui.set_opt_start_menu(start_menu_lnk().is_file());
        ui.set_opt_desktop(desktop_lnk().is_file());
        ui.set_opt_launch(true);
    }

    {
        use slint::winit_030::{winit, WinitWindowAccessor};
        let w = ui.as_weak();
        ui.on_drag(move || {
            if let Some(ui) = w.upgrade() {
                ui.window().with_winit_window(|win: &winit::window::Window| {
                    let _ = win.drag_window();
                });
            }
        });
        let w = ui.as_weak();
        ui.on_minimize(move || {
            if let Some(ui) = w.upgrade() {
                ui.window().set_minimized(true);
            }
        });
    }
    ui.on_close_window(|| {
        let _ = slint::quit_event_loop();
    });

    let w = ui.as_weak();
    ui.on_run(move || {
        let Some(ui) = w.upgrade() else { return };
        let o = Opts { start_menu: ui.get_opt_start_menu(), desktop: ui.get_opt_desktop(), restore: ui.get_opt_restore(), wipe: ui.get_opt_wipe() };
        ui.set_stage(1);
        ui.set_progress(0.0);
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let rw = weak.clone();
            let report: Report = Box::new(move |p, s| {
                let rw = rw.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = rw.upgrade() {
                        ui.set_progress(p);
                        if !s.is_empty() {
                            ui.set_status(s.into());
                        }
                    }
                });
            });
            let r = if uninstall_mode { uninstall(&o, es, &report) } else { install(&o, es, &report) };
            std::thread::sleep(std::time::Duration::from_millis(250));
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    match r {
                        Ok(m) => {
                            ui.set_status(m.into());
                            ui.set_stage(2);
                            // actualización: abrir AeTint y cerrar solo
                            if update_mode {
                                let w = ui.as_weak();
                                slint::Timer::single_shot(std::time::Duration::from_millis(700), move || {
                                    if let Some(ui) = w.upgrade() {
                                        ui.invoke_finish();
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            ui.set_status(e.into());
                            ui.set_stage(3);
                        }
                    }
                }
            });
        });
    });

    let w = ui.as_weak();
    ui.on_finish(move || {
        if let Some(ui) = w.upgrade() {
            if ui.get_stage() == 2 {
                if uninstall_mode {
                    delete_install_dir_after_exit();
                } else if ui.get_opt_launch() {
                    let _ = Command::new(install_dir().join("AeTint.exe")).spawn();
                }
            }
        }
        let _ = slint::quit_event_loop();
    });

    // se muestra, se centra en su monitor y arranca el bucle
    center_window(ui.window(), 640.0, 460.0);
    ui.show()?;
    if update_mode {
        let w = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(400), move || {
            if let Some(ui) = w.upgrade() {
                ui.invoke_run();
            }
        });
    }
    slint::run_event_loop()
}


/// Centra la ventana en el monitor donde está el ratón (las ventanas sin marco aparecen arriba a la izquierda).
/// Se llama antes de mostrarla, con su tamaño lógico.
fn center_window(w: &slint::Window, width: f32, height: f32) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
        use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return;
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let scale = dx as f32 / 96.0;
        let r = mi.rcWork;
        let x = r.left + ((r.right - r.left) - (width * scale) as i32) / 2;
        let y = r.top + ((r.bottom - r.top) - (height * scale) as i32) / 2;
        w.set_position(slint::PhysicalPosition::new(x.max(r.left), y.max(r.top)));
    }
}
