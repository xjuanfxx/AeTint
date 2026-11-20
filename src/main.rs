// AeTint — temas completos para After Effects.
//   · dvaui.dll        → paneles, botones, campos, listas (tablas de color float RGBA)
//   · AfterFXLib.dll   → timeline: regla, navegador, área de trabajo, CTI, barras de capa… (JSON de AE)
//   · menús Win32      → aetint_menu.dll pinta File/Edit/… y la barra de título dentro de AE
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod aelib;
mod color;
mod dvaui;
mod drover;
mod install;
mod jsonfit;
mod menus;
mod pe;
mod remap;
mod sound;
mod splash;
mod dialog;
mod update;
mod uxp;

use i18n::t;
use install::{Config, FileKind, SavedTheme, Target};
use remap::{Palette, ToneRef};
use serde_json::Value;
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

slint::include_modules!();

const AE_DEFAULTS: &str = include_str!("../assets/ae-colors-default.json");

/// Presets: nombre, paleta oscura (Darkest y Dark) y paleta clara (Light).
fn presets() -> Vec<(&'static str, Palette, Palette)> {
    let p = |b: &str, pa: &str, f: &str, r: &str, t: &str, a: &str| Palette {
        base: b.into(),
        panel: pa.into(),
        frame: f.into(),
        raised: r.into(),
        text: t.into(),
        accent: a.into(),
    };
    vec![
        // Catppuccin Latte
        ("Catppuccin Mocha", p("#11111B", "#1E1E2E", "#313244", "#181825", "#CDD6F4", "#7A80C2"), p("#DCE0E8", "#EFF1F5", "#CCD0DA", "#E6E9EF", "#4C4F69", "#7287FD")),
        // Tokyo Night Day
        ("Tokyo Night", p("#101014", "#1A1B26", "#292E42", "#16161E", "#C0CAF5", "#7AA2F7"), p("#CBD0DF", "#E1E2E7", "#B9BDCF", "#D6D8E1", "#3760BF", "#2E7DE9")),
        // Rosé Pine Dawn
        ("Rosé Pine", p("#121019", "#1F1D2E", "#26233A", "#191724", "#E0DEF4", "#C4A7E7"), p("#F2E9E1", "#FAF4ED", "#DFDAD9", "#F6EEE7", "#575279", "#907AA9")),
        // Nord Snow Storm
        ("Nord", p("#20242C", "#2E3440", "#3B4252", "#272C36", "#D8DEE9", "#88C0D0"), p("#D8DEE9", "#ECEFF4", "#C8D0DC", "#E5E9F0", "#2E3440", "#5E81AC")),
        // Gruvbox Light
        ("Gruvbox", p("#141617", "#282828", "#3C3836", "#1D2021", "#EBDBB2", "#D79921"), p("#EBDBB2", "#FBF1C7", "#D5C4A1", "#F2E5BC", "#3C3836", "#B57614")),
        // Alucard
        ("Dracula", p("#191A21", "#282A36", "#44475A", "#21222C", "#F8F8F2", "#BD93F9"), p("#DEDCCF", "#FFFBEB", "#CFCFDE", "#F5F1E0", "#1F1F1F", "#644AC9")),
        // One Light
        ("One Dark", p("#181A1F", "#282C34", "#3E4451", "#21252B", "#ABB2BF", "#61AFEF"), p("#E5E5E6", "#FAFAFA", "#D4D4D6", "#F0F0F1", "#383A42", "#4078F2")),
        ("Monokai", p("#1E1F1C", "#272822", "#3E3D32", "#22231E", "#F8F8F2", "#A6E22E"), p("#E8E6DA", "#FAFAF8", "#D6D4C8", "#F2F1EC", "#272822", "#6A9C10")),
        ("Solarized", p("#00212B", "#002B36", "#073642", "#00252F", "#93A1A1", "#268BD2"), p("#EEE8D5", "#FDF6E3", "#E0DAC6", "#F5EFDC", "#586E75", "#268BD2")),
        ("GitHub", p("#010409", "#0D1117", "#21262D", "#161B22", "#E6EDF3", "#2F81F7"), p("#EAEEF2", "#FFFFFF", "#D0D7DE", "#F6F8FA", "#1F2328", "#0969DA")),
        ("Everforest", p("#1E2326", "#2D353B", "#3D484D", "#272E33", "#D3C6AA", "#A7C080"), p("#EFEBD4", "#FDF6E3", "#E0DCC7", "#F4F0D9", "#5C6A72", "#8DA101")),
        // Kanagawa Lotus
        ("Kanagawa", p("#16161D", "#1F1F28", "#363646", "#1A1A22", "#DCD7BA", "#7E9CD8"), p("#E5DDB0", "#F2ECBC", "#DCD5AC", "#E7DBA0", "#545464", "#4D699B")),
        // Ayu Mirage / Ayu Light
        ("Ayu", p("#171B24", "#1F2430", "#2A3040", "#1C212B", "#CCCAC2", "#FFCC66"), p("#E7EAED", "#FCFCFC", "#D8DCE0", "#F3F4F5", "#5C6166", "#FA8D3E")),
        (t("Graphite", "Grafito"), p("#0A0A0A", "#171717", "#2A2A2A", "#101010", "#D4D4D4", "#E5484D"), p("#E4E4E4", "#F7F7F7", "#D2D2D2", "#EFEFEF", "#262626", "#E5484D")),
        // los grises de Adobe: aplicarlo deja After Effects como viene
        (t("AE original", "Original de AE"), p("#000000", "#1D1D1D", "#343434", "#0E0E0E", "#B0B0B0", "#066CE7"), p("#E6E6E6", "#FFFFFF", "#E6E6E6", "#F8F8F8", "#505050", "#0265DC")),
    ]
}

const PALETTE_ROWS: &[(&str, &str, &str, &str, &str)] = &[
    ("base", "Background", "Fondo", "Space between panels", "Espacio entre paneles"),
    ("panel", "Panels", "Paneles", "Inside every panel", "Fondo de cada panel"),
    ("frame", "Frames", "Marcos", "Tabs, toolbar, menu", "Pestañas, barra de herramientas, menú"),
    ("raised", "Fields", "Campos", "Search boxes, fields, viewer", "Buscadores, campos, visor"),
    ("text", "Text", "Texto", "Labels and values", "Etiquetas y valores"),
    ("accent", "Accent", "Acento", "Selection, buttons, focus", "Selección, botones, foco"),
];

fn group_label(id: &str, en: &'static str) -> &'static str {
    match id {
        "navigator" => t(en, "Navegador"),
        "workarea" => t(en, "Área de trabajo"),
        "cti" => t(en, "Cabezal (CTI)"),
        "layersel" => t(en, "Capa activa"),
        _ => en,
    }
}

// ── transición de colores: al cambiar entre oscuro y claro o al cargar un tema, la ventana pasa de unos colores a
// otros en un fundido corto en vez de saltar. Mientras se arrastra un color se cambia al instante.
thread_local! {
    static FADE_NEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FADE: RefCell<Option<(Preview, Preview, std::time::Instant)>> = const { RefCell::new(None) };
    static FADE_TIMER: slint::Timer = slint::Timer::default();
}

/// La próxima actualización de colores se hace con transición.
fn fade_next() {
    FADE_NEXT.with(|f| f.set(true));
}

const FADE_MS: f32 = 320.0;

fn mix_preview(a: &Preview, b: &Preview, t: f32) -> Preview {
    // Color::mix(otro, f) da f del propio color: aquí t es cuánto se ha avanzado hacia b
    let m = |x: Color, y: Color| y.mix(&x, t);
    macro_rules! all {
        ($($f:ident),*) => { Preview { $($f: m(a.$f, b.$f)),* } };
    }
    all!(base, panel, frame, raised, text, muted, accent, on_accent, tl_bg, grid, ruler, ruler_border, nav, work, work_edge, cti, layer, layer_sel, key_sel)
}

fn show_preview(ui: &AppWindow, target: Preview) {
    let g = ui.global::<Ui>();
    if !FADE_NEXT.with(|f| f.replace(false)) {
        FADE.with(|f| *f.borrow_mut() = None);
        FADE_TIMER.with(|t| t.stop());
        g.set_p(target);
        return;
    }
    let from = g.get_p();
    FADE.with(|f| *f.borrow_mut() = Some((from, target, std::time::Instant::now())));
    let weak = ui.as_weak();
    FADE_TIMER.with(|t| {
        t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(8), move || {
            let Some(ui) = weak.upgrade() else { return };
            let step = FADE.with(|f| {
                let f = f.borrow();
                let (a, b, t0) = f.as_ref()?;
                let x = (t0.elapsed().as_secs_f32() * 1000.0 / FADE_MS).min(1.0);
                // ease-in-out cúbica
                let e = if x < 0.5 { 4.0 * x * x * x } else { 1.0 - (-2.0 * x + 2.0).powi(3) / 2.0 };
                Some((if x >= 1.0 { b.clone() } else { mix_preview(a, b, e) }, x >= 1.0))
            });
            match step {
                Some((p, done)) => {
                    ui.global::<Ui>().set_p(p);
                    if done {
                        FADE.with(|f| *f.borrow_mut() = None);
                        FADE_TIMER.with(|t| t.stop());
                    }
                }
                None => FADE_TIMER.with(|t| t.stop()),
            }
        });
    });
}

fn slint_color(c: color::Rgb) -> Color {
    let [r, g, b] = color::to_u8(c);
    Color::from_rgb_u8(r, g, b)
}

fn pal_get<'a>(p: &'a Palette, k: &str) -> &'a String {
    match k {
        "base" => &p.base,
        "panel" => &p.panel,
        "frame" => &p.frame,
        "raised" => &p.raised,
        "text" => &p.text,
        _ => &p.accent,
    }
}
fn pal_set(p: &mut Palette, k: &str, v: String) {
    match k {
        "base" => p.base = v,
        "panel" => p.panel = v,
        "frame" => p.frame = v,
        "raised" => p.raised = v,
        "text" => p.text = v,
        _ => p.accent = v,
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> color::Rgb {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}
fn rgb_to_hsv([r, g, b]: color::Rgb) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

struct App {
    cfg: Config,
    /// se está editando la paleta clara (Light) en vez de la oscura
    edit_light: bool,
    themes: Vec<SavedTheme>,
    targets: Vec<Target>,
    status: HashMap<String, String>,
    defaults: Value,
    /// la última tarea (Some(true) = restaurar), para el botón "Retry" del aviso de estado
    last_restore: Option<bool>,
}

impl App {
    fn selected(&self) -> Vec<Target> {
        self.targets.iter().filter(|t| self.cfg.selected.contains(&t.id)).cloned().collect()
    }
}

/// El botón del aviso de estado: "Retry" tras un error, "Apply" si hay cambios sin aplicar.
fn status_action(ui: &AppWindow, app: &App) {
    let a = if ui.get_busy() {
        ""
    } else if ui.get_status_error() && app.last_restore.is_some() {
        t("Retry", "Reintentar")
    } else {
        ""
    };
    ui.set_status_action(a.into());
}

#[cfg(windows)]
fn round_corners(w: &slint::winit_030::winit::window::Window) {
    use slint::winit_030::winit::platform::windows::{CornerPreference, WindowExtWindows};
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    // Esquinas redondeadas de Windows 11. Sin `set_undecorated_shadow`: con esa sombra, en ventana (no maximizada)
    // la imagen se mostraba 1 px aplastada en vertical y todo quedaba borroso a medio píxel.
    w.set_corner_preference(CornerPreference::Round);
    if let Ok(h) = w.window_handle() {
        if let RawWindowHandle::Win32(h) = h.as_raw() {
            let pref: u32 = 2; // DWMWCP_ROUND
            unsafe {
                windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(h.hwnd.get() as _, 33, &pref as *const u32 as *const _, 4);
            }
        }
    }
}
#[cfg(not(windows))]
fn round_corners(_: &slint::winit_030::winit::window::Window) {}

/// Quita el borde de 1 px que Windows 11 dibuja alrededor de la ventana (una línea gris del sistema que no
/// casa con el tema); las esquinas redondeadas y la sombra se mantienen.
fn window_border(ui: &AppWindow) {
    #[cfg(windows)]
    {
        use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use slint::winit_030::{winit, WinitWindowAccessor};
        let rgb: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE
        ui.window().with_winit_window(|w: &winit::window::Window| {
            if let Ok(h) = w.window_handle() {
                if let RawWindowHandle::Win32(h) = h.as_raw() {
                    unsafe {
                        // DWMWA_BORDER_COLOR
                        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(h.hwnd.get() as _, 34, &rgb as *const u32 as *const _, 4);
                    }
                }
            }
        });
    }
}

/// Renderizador por software de Slint en todas las escalas: sin GPU, poca memoria y nítido también con escala
/// fraccionaria (125 %, 150 %). `SLINT_BACKEND` puesto a mano manda.
fn pick_renderer() {
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }
}

/// ¿El tema es el de After Effects tal cual (preset «AE original»)? Entonces los menús y la barra de título
/// se dejan como los pinta Windows: la DLL no toca nada.
fn is_native(dark: &Palette, light: &Palette) -> bool {
    presets().into_iter().last().is_some_and(|(_, d, l)| d == *dark && l == *light)
}

/// Los menús se pintan si están activados y el tema no es el original de AE.
fn menus_on(cfg: &Config) -> bool {
    cfg.menus && !is_native(&cfg.palette, &cfg.light)
}

fn idle_status() -> &'static str {
    t("Pick your colours and press Apply. After Effects must be closed.", "Elige colores y pulsa Aplicar. After Effects debe estar cerrado.")
}

/// Importa un archivo .aetint: lo añade a "My themes" (o reemplaza el del mismo nombre) y lo carga.
fn import_file(ui: &AppWindow, app: &Rc<RefCell<App>>, changed: &dyn Fn(), path: &std::path::Path) {
    match install::import_theme(path) {
        Ok(t) => {
            {
                let mut a = app.borrow_mut();
                match a.themes.iter_mut().find(|x| x.name == t.name) {
                    Some(x) => *x = t.clone(),
                    None => a.themes.push(t.clone()),
                }
                install::save_themes(&a.themes);
                a.cfg.palette = t.palette.clone();
                if let Some(l) = t.light.clone() {
                    a.cfg.light = l;
                }
                a.cfg.overrides = t.overrides.clone();
                fade_next();
                ui.set_themes(theme_items(&a.themes, a.edit_light));
            }
            changed();
            ui.set_lib_tab(1);
            ui.set_status(tr!("Imported “{}”. Press Apply theme to use it in After Effects.", "“{}” importado. Pulsa Aplicar tema para usarlo en After Effects.", t.name).into());
            ui.set_status_error(false);
            ui.set_status_ok(true);
        }
        Err(e) => {
            install::log_event(&format!("Import {} → ERROR: {e}", path.display()));
            ui.set_status(e.into());
            ui.set_status_error(true);
            ui.set_status_ok(false);
        }
    }
}

/// Doble clic en un .aetint abre AeTint: asociación en HKCU (sin permisos), solo desde una instalación real.
fn register_theme_files() {
    #[cfg(windows)]
    std::thread::spawn(|| {
        use std::os::windows::process::CommandExt;
        let Ok(exe) = std::env::current_exe() else { return };
        let exe_s = exe.display().to_string();
        if exe_s.to_lowercase().contains("\target") {
            return; // compilaciones de desarrollo
        }
        let cmd = format!("\"{exe_s}\" \"%1\"");
        let key = r"HKCU\Software\Classes\AeTint.Theme\shell\open\command";
        let cur = std::process::Command::new("reg").args(["query", key, "/ve"]).creation_flags(0x0800_0000).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        if cur.contains(&cmd) {
            return;
        }
        let add = |k: &str, v: &str| {
            let _ = std::process::Command::new("reg").args(["add", k, "/ve", "/d", v, "/f"]).creation_flags(0x0800_0000).status();
        };
        add(r"HKCU\Software\Classes\.aetint", "AeTint.Theme");
        add(r"HKCU\Software\Classes\AeTint.Theme", "AeTint theme");
        add(r"HKCU\Software\Classes\AeTint.Theme\DefaultIcon", &format!("\"{exe_s}\",0"));
        add(key, &cmd);
        unsafe {
            use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
            SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null());
        }
    });
}

/// Miniatura de la imagen del splash para el botón.
fn splash_thumb(cfg: &Config) -> slint::Image {
    let Some(p) = cfg.splash.as_deref() else { return slint::Image::default() };
    let Ok(img) = splash::load(std::path::Path::new(p)) else { return slint::Image::default() };
    let t = img.thumbnail(104, 64).to_rgba8();
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(t.as_raw(), t.width(), t.height());
    slint::Image::from_rgba8(buf)
}

fn splash_label(cfg: &Config) -> String {
    cfg.splash.as_deref().filter(|p| std::path::Path::new(p).is_file()).map(|_| i18n::t("Your image", "Tu imagen").to_string()).unwrap_or_default()
}

fn theme_items(themes: &[SavedTheme], light: bool) -> ModelRc<PresetItem> {
    let c = |h: &str| slint_color(color::parse_hex(h).unwrap_or([0.0; 3]));
    ModelRc::new(VecModel::from(
        themes
            .iter()
            .map(|t| {
                let p = if light { t.light.as_ref().unwrap_or(&t.palette) } else { &t.palette };
                PresetItem { name: t.name.clone().into(), base: c(&p.base), panel: c(&p.panel), raised: c(&p.raised), text: c(&p.text), accent: c(&p.accent) }
            })
            .collect::<Vec<_>>(),
    ))
}

/// Fichas de los presets, con los colores del brillo que se está editando.
fn preset_items(light: bool) -> ModelRc<PresetItem> {
    let c = |h: &str| slint_color(color::parse_hex(h).unwrap_or([0.0; 3]));
    ModelRc::new(VecModel::from(
        presets()
            .iter()
            .map(|(n, d, l)| (n, if light { l } else { d }))
            .map(|(n, p)| PresetItem { name: (*n).into(), base: c(&p.base), panel: c(&p.panel), raised: c(&p.raised), text: c(&p.text), accent: c(&p.accent) })
            .collect::<Vec<_>>(),
    ))
}

/// Actualiza las filas en su sitio cuando la lista conserva su tamaño: si se reemplazara el modelo,
/// Slint recrearía cada fila y cerraría el selector de color abierto mientras se arrastra.
fn set_rows<T: Clone + PartialEq + 'static>(cur: ModelRc<T>, rows: Vec<T>, replace: impl FnOnce(ModelRc<T>)) {
    if let Some(vm) = cur.as_any().downcast_ref::<VecModel<T>>() {
        if vm.row_count() == rows.len() {
            for (i, r) in rows.into_iter().enumerate() {
                if vm.row_data(i).as_ref() != Some(&r) {
                    vm.set_row_data(i, r);
                }
            }
            return;
        }
    }
    replace(ModelRc::new(VecModel::from(rows)));
}

fn refresh(ui: &AppWindow, app: &App) {
    let editing = if app.edit_light { &app.cfg.light } else { &app.cfg.palette };
    let Ok(pal) = editing.resolve() else { return };
    let (block, tone) = if app.edit_light { ("light", ToneRef::ae_light()) } else { ("darkest", ToneRef::ae_darkest()) };
    let pv = aelib::preview(&app.defaults, block, &pal, &tone, &app.cfg.overrides);
    ui.set_edit_light(app.edit_light);
    let get = |k: &str, fb: color::Rgb| pv.get(k).copied().unwrap_or(fb);
    let on_accent = if color::contrast(pal.accent, [0.0; 3]) > color::contrast(pal.accent, [1.0; 3]) { [0.05; 3] } else { [1.0; 3] };
    show_preview(ui, Preview {
        base: slint_color(pal.base),
        panel: slint_color(pal.panel),
        frame: slint_color(pal.frame),
        raised: slint_color(pal.raised),
        text: slint_color(pal.text),
        muted: slint_color(color::mix(pal.text, pal.panel, 0.42)),
        accent: slint_color(pal.accent),
        on_accent: slint_color(on_accent),
        tl_bg: slint_color(get("timelinegebackground", pal.panel)),
        grid: slint_color(get("timelinegegrid-line", pal.base)),
        ruler: slint_color(get("time-ruler-lower-background", pal.panel)),
        ruler_border: slint_color(get("time-ruler-tob-bottom-border", pal.base)),
        nav: slint_color(get("time-ruler-abs-grad-start", pal.frame)),
        work: slint_color(get("time-ruler-work-area-grad-start", pal.frame)),
        work_edge: slint_color(get("time-ruler-work-area-top-edge", pal.text)),
        cti: slint_color(get("time-ruler-cti-comp", [1.0, 0.0, 0.0])),
        layer: slint_color(get("tl-layer-bar-norm-grad-start", pal.frame)),
        layer_sel: slint_color(get("tl-layer-bar-sel-grad-start", pal.frame)),
        key_sel: slint_color(get("timelinegekeyframe-selected", [1.0, 0.93, 0.0])),
    });

    let rows: Vec<ColorEntry> = PALETTE_ROWS
        .iter()
        .map(|(k, en, es, hen, hes)| {
            let (label, hint) = (t(en, es), t(hen, hes));
            let hex = pal_get(editing, k).to_uppercase();
            ColorEntry {
                key: (*k).into(),
                label: label.into(),
                hint: hint.into(),
                value: slint_color(color::parse_hex(&hex).unwrap_or([0.0; 3])),
                hex: hex.into(),
                custom: true,
            }
        })
        .collect();
    set_rows(ui.get_palette(), rows, |m| ui.set_palette(m));

    let tl: Vec<ColorEntry> = aelib::GROUPS
        .iter()
        .map(|(id, label, keys)| {
            let custom = app.cfg.overrides.contains_key(keys[0]);
            let c = app.cfg.overrides.get(keys[0]).and_then(|h| color::parse_hex(h)).unwrap_or_else(|| get(keys[0], [0.5; 3]));
            ColorEntry {
                key: (*id).into(),
                label: group_label(id, label).into(),
                hint: format!("ae-color-{}", keys[0]).into(),
                hex: color::to_hex(c).into(),
                value: slint_color(c),
                custom,
            }
        })
        .collect();
    set_rows(ui.get_timeline(), tl, |m| ui.set_timeline(m));

    // las versiones más nuevas primero: con muchas instaladas la lista se desplaza y arriba quedan las de uso diario
    let ts: Vec<TargetItem> = app
        .targets
        .iter()
        .rev()
        .map(|t| TargetItem {
            id: t.id.clone().into(),
            label: t.label.replacen("After Effects", "AE", 1).into(),
            status: app.status.get(&t.id).cloned().unwrap_or_else(|| i18n::t("Checking…", "Comprobando…").into()).into(),
            selected: app.cfg.selected.contains(&t.id),
        })
        .collect();
    ui.set_targets(ModelRc::new(VecModel::from(ts)));
    status_action(ui, app);
}

/// Estado de cada instalación (en segundo plano: hay que leer ~80 MB por versión).
fn scan_status(ui: &AppWindow, targets: Vec<Target>) {
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let st: install::State = serde_json::from_slice(&std::fs::read(install::data_dir().join("state.json")).unwrap_or_default()).unwrap_or_default();
        let mut out = Vec::new();
        for t in &targets {
            let mut themed = 0;
            let mut foreign = false;
            let mut ver = String::new();
            for f in [FileKind::Dvaui, FileKind::AeLib] {
                let Ok(b) = std::fs::read(t.file(f)) else { continue };
                if st.written.get(&format!("{}/{}", t.id, f.name())) == Some(&install::sha(&b)) {
                    themed += 1;
                } else if f == FileKind::Dvaui && dvaui::Dvaui::open(&b).is_err() {
                    foreign = true;
                }
                // la versión de After Effects es la de AfterFXLib.dll (en 2020/2021 dvaui.dll lleva otra numeración,
                // la del framework compartido con Premiere: 14.9 y 15.4)
                if f == FileKind::AeLib {
                    if let Ok(p) = pe::Pe::parse(&b) {
                        ver = pe::file_version(&b, &p.resources(&b));
                    }
                }
            }
            let s = match (themed, foreign) {
                // cortos: caben en la tarjeta con la ventana en su tamaño normal
                (2, _) => i18n::t("Themed", "Con tema"),
                (1, _) => i18n::t("Partly themed", "Tema parcial"),
                (_, true) => i18n::t("Other app's theme", "Tema de otro programa"),
                _ => i18n::t("Original", "Original"),
            };
            // la versión, solo mayor.menor ("26.5.3.9" → "26.5")
            let short: Vec<&str> = ver.split('.').take(2).collect();
            out.push((t.id.clone(), if ver.is_empty() { s.to_string() } else { format!("{s} · v{}", short.join(".")) }));
        }
        let _ = weak.upgrade_in_event_loop(move |ui| {
            APP.with(|a| {
                if let Some(app) = a.borrow().as_ref() {
                    let mut app = app.borrow_mut();
                    for (id, s) in out {
                        app.status.insert(id, s);
                    }
                    refresh(&ui, &app);
                }
            });
        });
    });
}

thread_local! {
    static APP: RefCell<Option<Rc<RefCell<App>>>> = const { RefCell::new(None) };
}

fn run_job(ui: &AppWindow, app: &Rc<RefCell<App>>, restore: bool) {
    let (targets, cfg) = {
        let a = app.borrow();
        (a.selected(), a.cfg.clone())
    };
    if targets.is_empty() {
        ui.set_status(t("Tick at least one installation.", "Marca al menos una instalación.").into());
        ui.set_status_error(true);
        ui.set_status_ok(false);
        return;
    }
    app.borrow_mut().last_restore = Some(restore);
    ui.set_busy(true);
    ui.set_status_action("".into());
    ui.set_status_error(false);
    ui.set_status_ok(false);
    ui.set_status(if restore { t("Restoring…", "Restaurando…") } else { t("Applying…", "Aplicando…") }.into());
    let weak = ui.as_weak();
    let all = app.borrow().targets.clone();
    let names: Vec<String> = targets.iter().map(|t| t.label.clone()).collect();
    install::log_event(&format!("{} → {}", if restore { "Restore" } else { "Apply" }, names.join(", ")));
    std::thread::spawn(move || {
        let r = if restore { install::restore(&targets) } else { install::apply(&targets, &cfg) };
        match &r {
            Ok(m) => install::log_event(&format!("  ok: {m}")),
            Err(e) => install::log_event(&format!("  ERROR: {e}")),
        }
        if cfg.sounds {
            sound::play(if r.is_ok() { sound::Sound::Done } else { sound::Sound::Error });
        }
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_busy(false);
            match r {
                Ok(m) => {
                    ui.set_status(m.into());
                    ui.set_status_error(false);
                    ui.set_status_ok(true);
                }
                Err(e) => {
                    ui.set_status(e.into());
                    ui.set_status_error(true);
                    ui.set_status_ok(false);
                }
            }
            scan_status(&ui, all);
        });
    });
}

fn main() -> Result<(), slint::PlatformError> {
    let args: Vec<String> = std::env::args().collect();
    // modo administrador: copia los archivos preparados y termina
    if let Some(i) = args.iter().position(|a| a == "--install") {
        let code = args.get(i + 1).map(|m| install::run_install(std::path::Path::new(m))).unwrap_or(1);
        std::process::exit(code);
    }
    // desinstalador: devuelve todas las versiones de AE a sus originales, quita el plugin y el inicio con Windows
    if args.iter().any(|a| a == "--restore-all") {
        let cfg = install::load_config();
        i18n::set_lang(&cfg.lang);
        let _ = menus::set_autostart(false);
        let r = install::restore(&install::find_targets());
        if let Err(e) = &r {
            eprintln!("{e}");
        }
        std::process::exit(if r.is_ok() { 0 } else { 1 });
    }
    // vigilante de menús: carga aetint_menu.dll en cada After Effects que se abra
    if args.iter().any(|a| a == "--watch") {
        #[cfg(windows)]
        std::process::exit(menus::watch());
    }
    // desarrollo: cargar la DLL en un proceso concreto
    if let Some(i) = args.iter().position(|a| a == "--inject") {
        let pid: u32 = args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(0);
        let r = menus::dll_path().ok_or_else(|| tr!("aetint_menu.dll is missing next to the .exe", "falta aetint_menu.dll junto al .exe")).and_then(|d| menus::inject(pid, &d));
        println!("{r:?}");
        std::process::exit(if r.is_ok() { 0 } else { 1 });
    }
    // desarrollo: analiza los DLL de otras versiones (subcarpetas con dvaui.dll y AfterFXLib.dll)
    if let Some(i) = args.iter().position(|a| a == "--probe") {
        let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| ".".into()));
        let cfg = install::load_config();
        let mut dirs: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            println!("== {}", d.display());
            install::probe(&d, &cfg);
        }
        return Ok(());
    }
    // modo prueba: fabrica los archivos con tema en una carpeta, sin instalar nada
    if let Some(i) = args.iter().position(|a| a == "--dry-run") {
        let out = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "dry-run".into()));
        let cfg = install::load_config();
        // con el estado real: así reconoce lo que ya tiene tema y no toma un tema por original
        let st = install::load_state();
        for t in install::find_targets() {
            match install::build(&t, &cfg, &st) {
                Ok(files) => {
                    for (dst, b) in files {
                        let p = out.join(&t.id).join(dst.file_name().unwrap());
                        let _ = std::fs::create_dir_all(p.parent().unwrap());
                        std::fs::write(&p, b).unwrap();
                        println!("{} → {}", t.label, p.display());
                    }
                }
                Err(e) => println!("{}: {e}", t.label),
            }
        }
        return Ok(());
    }

    pick_renderer();
    let ui = AppWindow::new()?;
    let mut cfg = install::load_config();
    i18n::set_lang(&cfg.lang);
    ui.global::<Ui>().set_es(i18n::es());
    let targets = install::find_targets();
    if cfg.selected.is_empty() {
        cfg.selected = targets.iter().map(|t| t.id.clone()).collect();
    }
    // AeTint abre con el tema que tiene instalado After Effects (la primera versión marcada)
    {
        let first = targets.iter().find(|t| cfg.selected.contains(&t.id));
        match first.map(install::installed_theme) {
            Some(install::Installed::Ours(a)) => {
                cfg.palette = a.palette;
                if let Some(l) = a.light {
                    cfg.light = l;
                }
                cfg.overrides = a.overrides;
            }
            Some(install::Installed::Original) => {
                if let Some((_, p, l)) = presets().into_iter().last() {
                    cfg.palette = p;
                    cfg.light = l;
                    cfg.overrides.clear();
                }
            }
            _ => {}
        }
    }
    let app = Rc::new(RefCell::new(App {
        cfg,
        themes: install::load_themes(),
        edit_light: false,
        targets: targets.clone(),
        status: HashMap::new(),
        defaults: serde_json::from_str(AE_DEFAULTS).unwrap_or(Value::Null),
        last_restore: None,
    }));
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));

    ui.set_presets(preset_items(false));
    ui.set_themes(theme_items(&app.borrow().themes, false));
    menus::deploy_dll();

    {
        let g = ui.global::<Ui>();
        g.on_hsv_color(|h, s, v| slint_color(hsv_to_rgb(h, s, v)));
        g.on_track_image(|kind, h, sl, l| {
            // HSL → RGB
            fn hsl(h: f32, s: f32, l: f32) -> color::Rgb {
                let v = l + s * l.min(1.0 - l);
                let sv = if v <= 0.0 { 0.0 } else { 2.0 * (1.0 - l / v) };
                hsv_to_rgb(h, sv, v)
            }
            const W: u32 = 128;
            let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(W, 1);
            for (x, px) in buf.make_mut_slice().iter_mut().enumerate() {
                let t = x as f32 / (W - 1) as f32;
                let c = match kind {
                    0 => hsv_to_rgb(t.min(0.999), 1.0, 1.0),
                    1 => hsl(h, t, l.clamp(0.15, 0.85)),
                    _ => hsl(h, sl, t),
                };
                let c = color::to_u8(c);
                *px = slint::Rgb8Pixel { r: c[0], g: c[1], b: c[2] };
            }
            slint::Image::from_rgb8(buf)
        });
        // engranaje girado: los dientes giran alrededor del centro (8,8); el agujero central es un círculo
        g.on_gear_path(|deg| {
            const PTS: [(f32, f32); 32] = [
                (8.00, 2.10), (9.48, 0.55), (10.91, 0.98), (11.28, 3.09), (12.17, 3.83), (14.32, 3.78), (15.02, 5.09), (13.79, 6.85),
                (13.90, 8.00), (15.45, 9.48), (15.02, 10.91), (12.91, 11.28), (12.17, 12.17), (12.22, 14.32), (10.91, 15.02), (9.15, 13.79),
                (8.00, 13.90), (6.52, 15.45), (5.09, 15.02), (4.72, 12.91), (3.83, 12.17), (1.68, 12.22), (0.98, 10.91), (2.21, 9.15),
                (2.10, 8.00), (0.55, 6.52), (0.98, 5.09), (3.09, 4.72), (3.83, 3.83), (3.78, 1.68), (5.09, 0.98), (6.85, 2.21),
            ];
            let (sn, cs) = deg.to_radians().sin_cos();
            let mut out = String::with_capacity(600);
            for (i, (x, y)) in PTS.iter().enumerate() {
                let (dx, dy) = (x - 8.0, y - 8.0);
                let (rx, ry) = (8.0 + dx * cs - dy * sn, 8.0 + dx * sn + dy * cs);
                out.push_str(&format!("{} {rx:.2} {ry:.2} ", if i == 0 { "M" } else { "L" }));
            }
            out.push_str("Z M 10.60 8 A 2.6 2.6 0 1 0 5.40 8 A 2.6 2.6 0 1 0 10.60 8 Z");
            out.into()
        });
        // luz del logo: una mancha radial ya coloreada, a la resolución real de la pantalla y con un tramado fino
        // (dithering) para que el degradado no haga escalones. Solo se genera la zona `x0,y0,w,h` (en px lógicos,
        // relativa al centro) que puede llegar a verse; se vuelve a generar al cambiar el color o la escala.
        // `kind` elige los cortes del degradado (0 halo, 1 reflejo, 2 núcleo, 3 velo sobre el contenido).
        {
            g.on_glow_image(move |peak, radius, kind, x0, y0, w, h, scale| {
                let scale = if scale > 0.0 { scale } else { 1.0 };
                let (pw, ph) = ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32);
                let stops: &[(f32, f32)] = match kind {
                    0 => &[(0.0, 1.0), (0.34, 0.346), (0.7, 0.0)],
                    1 => &[(0.0, 1.0), (0.7, 0.0)],
                    3 => &[(0.0, 1.0), (0.6, 0.0)],
                    _ => &[(0.0, 1.0), (0.4, 0.3), (0.7, 0.0)],
                };
                let profile = |r: f32| -> f32 {
                    for win in stops.windows(2) {
                        let ((r0, a0), (r1, a1)) = (win[0], win[1]);
                        if r <= r1 {
                            return a0 + (a1 - a0) * ((r - r0) / (r1 - r0)).clamp(0.0, 1.0);
                        }
                    }
                    0.0
                };
                // ruido triangular determinista por píxel, en [-1, 1]
                let noise = |x: u32, y: u32, k: u32| -> f32 {
                    let h = |mut v: u32| {
                        v ^= v >> 16;
                        v = v.wrapping_mul(0x7feb_352d);
                        v ^= v >> 15;
                        v = v.wrapping_mul(0x846c_a68b);
                        v ^ (v >> 16)
                    };
                    let a = h(x.wrapping_mul(1973) ^ y.wrapping_mul(9277) ^ k.wrapping_mul(26699)) as f32 / u32::MAX as f32;
                    let b = h(a.to_bits() ^ 0x9e37_79b9) as f32 / u32::MAX as f32;
                    a + b - 1.0
                };
                let (r, gc, b) = (255u8, 255u8, 255u8);
                let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(pw, ph);
                for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
                    let (ix, iy) = (i as u32 % pw, i as u32 / pw);
                    let lx = x0 + (ix as f32 + 0.5) / scale;
                    let ly = y0 + (iy as f32 + 0.5) / scale;
                    let f = profile((lx * lx + ly * ly).sqrt() / radius);
                    let a = if f > 0.0 { (peak * f * 255.0 + noise(ix, iy, kind as u32) * 1.6).clamp(0.0, 255.0) } else { 0.0 };
                    *px = slint::Rgba8Pixel { r, g: gc, b, a: a.round() as u8 };
                }
                slint::Image::from_rgba8(buf)
            });
        }
        // fondo del logo: cuadrado redondeado con el degradado del acento, a la resolución real de la pantalla
        // (el renderizador por software no redondea un relleno con degradado)
        {
            g.on_logo_image(move |accent, size, scale| {
                let scale = if scale > 0.0 { scale } else { 1.0 };
                let n = (size * scale).round().max(2.0) as u32;
                let (c0, c1, c2) = (accent.brighter(0.35), accent, accent.darker(0.45));
                let lerp = |a: slint::Color, b: slint::Color, t: f32| {
                    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
                    (m(a.red(), b.red()), m(a.green(), b.green()), m(a.blue(), b.blue()))
                };
                let rad = n as f32 * 0.28;
                let half = n as f32 / 2.0;
                let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(n, n);
                for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
                    let (x, y) = ((i as u32 % n) as f32 + 0.5, (i as u32 / n) as f32 + 0.5);
                    // distancia con signo al cuadrado redondeado: alfa con bordes suaves
                    let qx = (x - half).abs() - (half - rad);
                    let qy = (y - half).abs() - (half - rad);
                    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rad;
                    let a = (0.5 - d).clamp(0.0, 1.0);
                    let t = (x + y) / (2.0 * n as f32);
                    let (r, g, b) = if t < 0.5 { lerp(c0, c1, t * 2.0) } else { lerp(c1, c2, (t - 0.5) * 2.0) };
                    *px = slint::Rgba8Pixel { r, g, b, a: (a * 255.0).round() as u8 };
                }
                slint::Image::from_rgba8(buf)
            });
        }
        g.on_sv_image(|h| {
            // en un tono fijo el color varía de forma bilineal con S y V: 64×64 se escala sin que se note
            const N: u32 = 64;
            let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(N, N);
            let px = buf.make_mut_slice();
            for y in 0..N {
                for x in 0..N {
                    let c = color::to_u8(hsv_to_rgb(h, x as f32 / (N - 1) as f32, 1.0 - y as f32 / (N - 1) as f32));
                    px[(y * N + x) as usize] = slint::Rgb8Pixel { r: c[0], g: c[1], b: c[2] };
                }
            }
            slint::Image::from_rgb8(buf)
        });
        g.on_hsv_hex(|h, s, v| color::to_hex(hsv_to_rgb(h, s, v)).into());
        g.on_hex_hsv(|hx| {
            let (h, s, v) = rgb_to_hsv(color::parse_hex(&hx).unwrap_or([0.5; 3]));
            Hsv { h, s, v }
        });
    }

    let changed = {
        let weak = ui.as_weak();
        let app = app.clone();
        move || {
            let a = app.borrow();
            install::save_config(&a.cfg);
            if a.cfg.menus {
                let _ = menus::write_colors(&a.cfg.palette, &a.cfg.light, menus_on(&a.cfg));
            }
            if let Some(ui) = weak.upgrade() {
                refresh(&ui, &a);
            }
        }
    };
    let changed = Rc::new(changed);

    ui.on_set_palette({
        let (app, changed) = (app.clone(), changed.clone());
        move |k, hx| {
            if color::parse_hex(&hx).is_none() {
                return;
            }
            {
                let mut a = app.borrow_mut();
                let v = color::to_hex(color::parse_hex(&hx).unwrap());
                if a.edit_light {
                    pal_set(&mut a.cfg.light, &k, v);
                } else {
                    pal_set(&mut a.cfg.palette, &k, v);
                }
            }
            changed();
        }
    });
    ui.on_set_override({
        let (app, changed) = (app.clone(), changed.clone());
        move |id, hx| {
            let Some(c) = color::parse_hex(&hx) else { return };
            if let Some((_, _, keys)) = aelib::GROUPS.iter().find(|g| g.0 == id.as_str()) {
                let mut a = app.borrow_mut();
                for k in *keys {
                    a.cfg.overrides.insert((*k).to_string(), color::to_hex(c));
                }
            }
            changed();
        }
    });
    ui.on_toggle_override({
        let (app, changed, weak) = (app.clone(), changed.clone(), ui.as_weak());
        move |id, on| {
            if let Some((_, _, keys)) = aelib::GROUPS.iter().find(|g| g.0 == id.as_str()) {
                let mut a = app.borrow_mut();
                if on {
                    // empieza desde el color que tiene ahora en la vista previa
                    let cur = weak
                        .upgrade()
                        .and_then(|ui| ui.get_timeline().iter().find(|e| e.key == id).map(|e| e.hex.to_string()))
                        .unwrap_or_else(|| "#808080".into());
                    for k in *keys {
                        a.cfg.overrides.insert((*k).to_string(), cur.clone());
                    }
                } else {
                    for k in *keys {
                        a.cfg.overrides.remove(*k);
                    }
                }
            }
            changed();
        }
    });
    ui.on_use_preset({
        let (app, changed) = (app.clone(), changed.clone());
        move |i| {
            if let Some((_, p, l)) = presets().into_iter().nth(i as usize) {
                let mut a = app.borrow_mut();
                a.cfg.palette = p;
                a.cfg.light = l;
                drop(a);
                fade_next();
                changed();
            }
        }
    });
    ui.on_toggle_target({
        let (app, changed) = (app.clone(), changed.clone());
        move |id, on| {
            let mut a = app.borrow_mut();
            let id = id.to_string();
            a.cfg.selected.retain(|x| *x != id);
            if on {
                a.cfg.selected.push(id);
            }
            drop(a);
            changed();
        }
    });
    ui.on_toggle_menus({
        let (app, changed, weak) = (app.clone(), changed.clone(), ui.as_weak());
        move |on| {
            app.borrow_mut().cfg.menus = on;
            let (pal, light) = (app.borrow().cfg.palette.clone(), app.borrow().cfg.light.clone());
            // el vigilante solo hace falta si alguna versión de AE no tiene el plugin AeTint.aex
            let need_watcher = on && app.borrow().cfg.watcher;
            let r = menus::write_colors(&pal, &light, on && !is_native(&pal, &light)).and_then(|_| if need_watcher { menus::start_watcher() } else { Ok(()) });
            // menús desactivados: el vigilante ya no tiene nada que hacer
            if !on {
                menus::stop_watcher();
            }
            if let Some(ui) = weak.upgrade() {
                ui.set_menus(on);
                match (r, menus::dll_path()) {
                    (Err(e), _) => {
                        ui.set_status(e.into());
                        ui.set_status_error(true);
                        ui.set_status_ok(false);
                    }
                    (Ok(()), None) if on => {
                        ui.set_status(t("aetint_menu.dll is missing next to AeTint.exe.", "Falta aetint_menu.dll junto a AeTint.exe.").into());
                        ui.set_status_error(true);
                        ui.set_status_ok(false);
                    }
                    _ => {
                        ui.set_status(
                            if on {
                                t("Menus on: they change when After Effects opens (and live if it is already open).", "Menús activados: se aplican al abrir After Effects (y en vivo si ya está abierto).")
                            } else {
                                t("Menus off.", "Menús desactivados.")
                            }
                            .into(),
                        );
                        ui.set_status_error(false);
                        ui.set_status_ok(true);
                    }
                }
            }
            changed();
        }
    });
    ui.on_toggle_sounds({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |on| {
            app.borrow_mut().cfg.sounds = on;
            install::save_config(&app.borrow().cfg);
            if let Some(ui) = weak.upgrade() {
                ui.set_sounds(on);
            }
            if on {
                sound::play(sound::Sound::Done);
            }
        }
    });
    ui.on_toggle_watcher({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |on| {
            app.borrow_mut().cfg.watcher = on;
            install::save_config(&app.borrow().cfg);
            let r = if on && app.borrow().cfg.menus {
                menus::start_watcher()
            } else {
                menus::stop_watcher();
                let _ = menus::set_autostart(false);
                Ok(())
            };
            if let Some(ui) = weak.upgrade() {
                ui.set_watcher(on);
                ui.set_autostart(menus::autostart_enabled());
                match r {
                    Err(e) => {
                        ui.set_status(e.into());
                        ui.set_status_error(true);
                        ui.set_status_ok(false);
                    }
                    Ok(()) => {
                        ui.set_status(if on {
                            t("Watchdog on: it loads the menus into versions of After Effects without the AeTint plugin.", "Vigilante activado: carga los menús en las versiones de After Effects sin el plugin de AeTint.")
                        } else {
                            t("Watchdog off.", "Vigilante desactivado.")
                        }.into());
                        ui.set_status_error(false);
                        ui.set_status_ok(on);
                    }
                }
            }
        }
    });
    ui.on_toggle_autostart({
        let weak = ui.as_weak();
        move |on| {
            let r = menus::set_autostart(on);
            if let Some(ui) = weak.upgrade() {
                ui.set_autostart(menus::autostart_enabled());
                if let Err(e) = r {
                    ui.set_status(e.into());
                    ui.set_status_error(true);
                    ui.set_status_ok(false);
                }
            }
        }
    });
    ui.on_apply({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            if let Some(ui) = weak.upgrade() {
                run_job(&ui, &app, false);
            }
        }
    });
    ui.on_restore({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            if let Some(ui) = weak.upgrade() {
                run_job(&ui, &app, true);
            }
        }
    });
    // pulsar un error abre el registro completo
    ui.on_open_log(|| {
        let p = install::log_path();
        if !p.is_file() {
            install::log_event("(log created)");
        }
        let _ = std::process::Command::new("explorer").arg(&p).spawn();
    });
    // el botón del aviso de estado: repite la última tarea tras un error, o aplica los cambios
    ui.on_status_act({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_status_error() {
                    let again = app.borrow().last_restore.unwrap_or(false);
                    run_job(&ui, &app, again);
                } else {
                    run_job(&ui, &app, false);
                }
            }
        }
    });
    ui.on_set_lang({
        let (app, changed, weak) = (app.clone(), changed.clone(), ui.as_weak());
        move |es| {
            app.borrow_mut().cfg.lang = if es { "es" } else { "en" }.into();
            i18n::set_lang(&app.borrow().cfg.lang);
            if let Some(ui) = weak.upgrade() {
                ui.global::<Ui>().set_es(es);
                ui.set_presets(preset_items(app.borrow().edit_light));
                ui.set_status(idle_status().into());
                ui.set_status_error(false);
                ui.set_status_ok(false);
                let targets = app.borrow().targets.clone();
                app.borrow_mut().status.clear();
                changed();
                scan_status(&ui, targets);
            }
        }
    });
    {
        use slint::winit_030::{winit, WinitWindowAccessor};
        let weak = ui.as_weak();
        ui.on_start_drag(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w: &winit::window::Window| {
                    let _ = w.drag_window();
                });
            }
        });
        let weak = ui.as_weak();
        ui.on_start_resize(move |d| {
            use winit::window::ResizeDirection as D;
            let dir = match d {
                0 => D::North,
                1 => D::South,
                2 => D::East,
                3 => D::West,
                4 => D::NorthEast,
                5 => D::NorthWest,
                6 => D::SouthEast,
                _ => D::SouthWest,
            };
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w: &winit::window::Window| {
                    let _ = w.drag_resize_window(dir);
                });
            }
        });
        let weak = ui.as_weak();
        ui.on_win_minimize(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().set_minimized(true);
            }
        });
        let weak = ui.as_weak();
        ui.on_win_maximize(move || {
            if let Some(ui) = weak.upgrade() {
                let m = !ui.window().is_maximized();
                ui.window().set_maximized(m);
                ui.set_is_max(m);
            }
        });
        ui.on_win_close(|| {
            let _ = slint::quit_event_loop();
        });
        // escala de la pantalla: al arrancar y cada vez que la ventana pasa a otra pantalla (otra escala), para
        // que el logo y la luz se vuelvan a generar a la resolución real
        ui.global::<Ui>().set_scale(ui.window().scale_factor());
        {
            let weak = ui.as_weak();
            ui.window().on_winit_window_event(move |w, ev| {
                if let winit::event::WindowEvent::Resized(size) = ev {
                    let tall = size.height as f32 / w.scale_factor() >= 799.5;
                    let weak = weak.clone();
                    slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
                        if let Some(ui) = weak.upgrade() {
                            if ui.get_tall() != tall {
                                ui.set_tall(tall);
                            }
                        }
                    });
                }
                if let winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } = ev {
                    let s = *scale_factor as f32;
                    let weak = weak.clone();
                    slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
                        if let Some(ui) = weak.upgrade() {
                            ui.global::<Ui>().set_scale(s);
                        }
                    });
                }
                slint::winit_030::EventResult::Propagate
            });
        }
        // reloj de la luz del logo: 20 fps (se mueve despacio, no se nota) solo con la ventana activa y visible;
        // con el renderizador por software cada paso redibuja la ventana entera
        {
            let weak = ui.as_weak();
            let t = std::cell::Cell::new(0.0f32);
            let last = std::cell::Cell::new(std::time::Instant::now());
            let timer = slint::Timer::default();
            timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(50), move || {
                let now = std::time::Instant::now();
                let dt = now.duration_since(last.replace(now)).as_secs_f32().min(0.1);
                let Some(ui) = weak.upgrade() else { return };
                let live = ui
                    .window()
                    .with_winit_window(|w: &winit::window::Window| w.has_focus() && !w.is_minimized().unwrap_or(false))
                    .unwrap_or(true);
                if live {
                    t.set(t.get() + dt);
                    ui.set_lt(t.get());
                }
            });
            std::mem::forget(timer);
        }
        // esquinas redondeadas de Windows 11 en la ventana sin marco
        // se pide varias veces: si se pide antes de que la ventana termine de mostrarse, Windows lo ignora
        for ms in [50u64, 400, 1200] {
            let weak = ui.as_weak();
            slint::Timer::single_shot(std::time::Duration::from_millis(ms), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.window().with_winit_window(|w: &winit::window::Window| {
                        round_corners(w);
                    });
                    window_border(&ui);
                }
            });
        }
    }
    // ── brillo que se edita: Dark (Darkest y Dark de AE) o Light
    ui.on_set_edit_light({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |light| {
            app.borrow_mut().edit_light = light;
            fade_next();
            if let Some(ui) = weak.upgrade() {
                let a = app.borrow();
                ui.set_presets(preset_items(light));
                ui.set_themes(theme_items(&a.themes, light));
                refresh(&ui, &a);
            }
        }
    });
    // ── biblioteca de temas propios
    ui.on_save_theme({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |name| {
            let name = name.trim().to_string();
            if name.is_empty() {
                return;
            }
            let mut a = app.borrow_mut();
            let t = SavedTheme { name: name.clone(), palette: a.cfg.palette.clone(), light: Some(a.cfg.light.clone()), overrides: a.cfg.overrides.clone() };
            let replaced = if let Some(x) = a.themes.iter_mut().find(|x| x.name.to_lowercase() == name.to_lowercase()) {
                *x = t;
                true
            } else {
                a.themes.push(t);
                false
            };
            install::save_themes(&a.themes);
            if let Some(ui) = weak.upgrade() {
                ui.set_themes(theme_items(&a.themes, a.edit_light));
                ui.set_status(if replaced { tr!("Updated “{}”.", "“{}” actualizado.", name) } else { tr!("Saved “{}” in My themes.", "“{}” guardado en Mis temas.", name) }.into());
                ui.set_status_error(false);
                ui.set_status_ok(true);
            }
        }
    });
    ui.on_apply_theme({
        let (app, changed, weak) = (app.clone(), changed.clone(), ui.as_weak());
        move |i| {
            let Some(t) = app.borrow().themes.get(i as usize).cloned() else { return };
            {
                let mut a = app.borrow_mut();
                a.cfg.palette = t.palette;
                if let Some(l) = t.light {
                    a.cfg.light = l;
                }
                a.cfg.overrides = t.overrides;
            }
            fade_next();
            changed();
            if let Some(ui) = weak.upgrade() {
                ui.set_status(tr!("Loaded “{}”. Press Apply theme to use it in After Effects.", "“{}” cargado. Pulsa Aplicar tema para usarlo en After Effects.", t.name).into());
                ui.set_status_error(false);
                ui.set_status_ok(false);
            }
        }
    });
    ui.on_delete_theme({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i| {
            let mut a = app.borrow_mut();
            if (i as usize) >= a.themes.len() {
                return;
            }
            let t = a.themes.remove(i as usize);
            install::save_themes(&a.themes);
            if let Some(ui) = weak.upgrade() {
                ui.set_themes(theme_items(&a.themes, a.edit_light));
                ui.set_status(tr!("Deleted “{}”.", "“{}” borrado.", t.name).into());
                ui.set_status_error(false);
                ui.set_status_ok(false);
            }
        }
    });
    ui.on_import_theme({
        let (app, changed, weak) = (app.clone(), changed.clone(), ui.as_weak());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(p) = dialog::open(t("Import theme", "Importar tema"), &[(t("AeTint theme", "Tema de AeTint"), "*.aetint")]) {
                import_file(&ui, &app, &*changed, &p);
            }
        }
    });
    ui.on_export_theme({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let Some(th) = app.borrow().themes.get(i as usize).cloned() else { return };
            let file: String = th.name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect();
            let Some(p) = dialog::save(t("Export theme", "Exportar tema"), &[(t("AeTint theme", "Tema de AeTint"), "*.aetint")], &format!("{file}.aetint"), "aetint") else { return };
            match install::export_theme(&th, &p) {
                Ok(()) => {
                    ui.set_status(tr!("Exported “{}”. Double-click the file to open it in AeTint.", "“{}” exportado. Haz doble clic en el archivo para abrirlo en AeTint.", th.name).into());
                    ui.set_status_error(false);
                    ui.set_status_ok(true);
                }
                Err(e) => {
                    ui.set_status(e.into());
                    ui.set_status_error(true);
                    ui.set_status_ok(false);
                }
            }
        }
    });
    ui.on_choose_splash({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(p) = dialog::open(t("Splash screen image", "Imagen para la pantalla de inicio"), &[(t("Images", "Imágenes"), "*.png;*.jpg;*.jpeg")]) else { return };
            match install::store_splash(&p) {
                Ok(stored) => {
                    let mut a = app.borrow_mut();
                    a.cfg.splash = Some(stored);
                    install::save_config(&a.cfg);
                    ui.set_splash_name(splash_label(&a.cfg).into());
                    ui.set_splash_thumb(splash_thumb(&a.cfg));
                    ui.set_status(t("Splash image set. Press Apply theme to put it in After Effects.", "Imagen de inicio elegida. Pulsa Aplicar tema para ponerla en After Effects.").into());
                    ui.set_status_error(false);
                    ui.set_status_ok(true);
                }
                Err(e) => {
                    ui.set_status(e.into());
                    ui.set_status_error(true);
                    ui.set_status_ok(false);
                }
            }
        }
    });
    ui.on_clear_splash({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let mut a = app.borrow_mut();
            a.cfg.splash = None;
            install::save_config(&a.cfg);
            if let Some(ui) = weak.upgrade() {
                ui.set_splash_name("".into());
                ui.set_status(t("Adobe's splash again. Press Apply theme to put it back.", "Vuelve el splash de Adobe. Pulsa Aplicar tema para devolverlo.").into());
                ui.set_status_error(false);
                ui.set_status_ok(false);
            }
        }
    });
    ui.on_rescan({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let t = install::find_targets();
            app.borrow_mut().targets = t.clone();
            if let Some(ui) = weak.upgrade() {
                scan_status(&ui, t);
            }
        }
    });

    refresh(&ui, &app.borrow());
    ui.set_menus(app.borrow().cfg.menus);
    // sin vigilante no hace falta arrancar con Windows (versiones anteriores lo dejaban puesto)
    if !app.borrow().cfg.watcher {
        if menus::autostart_enabled() {
            let _ = menus::set_autostart(false);
        }
        if menus::watcher_running() {
            menus::stop_watcher();
        }
    }
    ui.set_watcher(app.borrow().cfg.watcher);
    ui.set_autostart(menus::autostart_enabled());
    ui.set_splash_name(splash_label(&app.borrow().cfg).into());
    ui.set_splash_thumb(splash_thumb(&app.borrow().cfg));
    register_theme_files();
    // doble clic en un .aetint: AeTint se abre con ese archivo
    if let Some(p) = std::env::args().skip(1).find(|a| a.to_lowercase().ends_with(".aetint")) {
        import_file(&ui, &app, &*changed, std::path::Path::new(&p));
    }
    ui.set_sounds(app.borrow().cfg.sounds);
    if app.borrow().cfg.menus {
        let _ = menus::write_colors(&app.borrow().cfg.palette, &app.borrow().cfg.light, menus_on(&app.borrow().cfg));
        if app.borrow().cfg.watcher {
            let _ = menus::start_watcher();
        }
    }
    ui.set_status(SharedString::from(idle_status()));
    scan_status(&ui, targets);

    // ── actualizaciones: la última Release de GitHub, en segundo plano; al pulsar el aviso se descarga
    // el instalador, se comprueba su huella y se actualiza solo
    ui.set_app_version(update::CURRENT.into());
    let pending: Rc<RefCell<Option<update::Update>>> = Rc::new(RefCell::new(None));
    ui.on_open_update({
        let (pending, weak) = (pending.clone(), ui.as_weak());
        move || {
            let Some(up) = pending.borrow().clone() else { return };
            let Some(ui) = weak.upgrade() else { return };
            if ui.get_update_busy() {
                return;
            }
            // sin instalador publicado (o versión portable antigua): se abre la página de la Release
            if up.setup_url.is_none() || up.sha_url.is_none() {
                update::open(&up.url);
                return;
            }
            ui.set_update_busy(true);
            ui.set_update_progress(0.0);
            ui.set_status(tr!("Downloading AeTint {}…", "Descargando AeTint {}…", up.version).into());
            ui.set_status_error(false);
            ui.set_status_ok(false);
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let w = weak.clone();
                let r = update::download(&up, move |p| {
                    let w = w.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = w.upgrade() {
                            ui.set_update_progress(p);
                        }
                    });
                })
                .and_then(|path| update::run_installer(&path));
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = weak.upgrade() else { return };
                    match r {
                        // el instalador cierra AeTint y lo vuelve a abrir actualizado
                        Ok(()) => {
                            ui.set_update_progress(1.0);
                            ui.set_status(i18n::t("Installing the update…", "Instalando la actualización…").into());
                            let _ = slint::quit_event_loop();
                        }
                        Err(e) => {
                            ui.set_update_busy(false);
                            ui.set_status(e.into());
                            ui.set_status_error(true);
                            update::open(&up.url);
                        }
                    }
                });
            });
        }
    });
    {
        let weak = ui.as_weak();
        let pending = pending.clone();
        let (tx, rx) = std::sync::mpsc::channel::<update::Update>();
        std::thread::spawn(move || {
            if let Some(up) = update::check() {
                let _ = tx.send(up);
            }
        });
        // el resultado llega a este hilo (el de la interfaz) por un temporizador corto
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(500), move || {
            if let Ok(up) = rx.try_recv() {
                if let Some(ui) = weak.upgrade() {
                    ui.set_update_version(up.version.clone().into());
                }
                *pending.borrow_mut() = Some(up);
            }
        });
        std::mem::forget(timer);
    }
    // se muestra, se centra en su monitor y arranca el bucle
    center_window(ui.window(), 1180.0, 800.0);
    ui.show()?;
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
