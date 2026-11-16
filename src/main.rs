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
mod install;
mod jsonfit;
mod menus;
mod pe;
mod remap;
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
}

impl App {
    fn selected(&self) -> Vec<Target> {
        self.targets.iter().filter(|t| self.cfg.selected.contains(&t.id)).cloned().collect()
    }
}

#[cfg(windows)]
fn round_corners(w: &slint::winit_030::winit::window::Window) {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
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

fn idle_status() -> &'static str {
    t("Pick your colours and press Apply. After Effects must be closed.", "Elige colores y pulsa Aplicar. After Effects debe estar cerrado.")
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

fn refresh(ui: &AppWindow, app: &App) {
    let editing = if app.edit_light { &app.cfg.light } else { &app.cfg.palette };
    let Ok(pal) = editing.resolve() else { return };
    let (block, tone) = if app.edit_light { ("light", ToneRef::ae_light()) } else { ("darkest", ToneRef::ae_darkest()) };
    let pv = aelib::preview(&app.defaults, block, &pal, &tone, &app.cfg.overrides);
    ui.set_edit_light(app.edit_light);
    let get = |k: &str, fb: color::Rgb| pv.get(k).copied().unwrap_or(fb);
    let on_accent = if color::contrast(pal.accent, [0.0; 3]) > color::contrast(pal.accent, [1.0; 3]) { [0.05; 3] } else { [1.0; 3] };
    ui.global::<Ui>().set_p(Preview {
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
    ui.set_palette(ModelRc::new(VecModel::from(rows)));

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
    ui.set_timeline(ModelRc::new(VecModel::from(tl)));

    let ts: Vec<TargetItem> = app
        .targets
        .iter()
        .map(|t| TargetItem {
            id: t.id.clone().into(),
            label: t.label.clone().into(),
            status: app.status.get(&t.id).cloned().unwrap_or_else(|| i18n::t("Checking…", "Comprobando…").into()).into(),
            selected: app.cfg.selected.contains(&t.id),
        })
        .collect();
    ui.set_targets(ModelRc::new(VecModel::from(ts)));
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
                if f == FileKind::Dvaui {
                    if let Ok(p) = pe::Pe::parse(&b) {
                        ver = pe::file_version(&b, &p.resources(&b));
                    }
                }
            }
            let s = match (themed, foreign) {
                (2, _) => i18n::t("AeTint theme", "Tema de AeTint"),
                (1, _) => i18n::t("AeTint theme (partial)", "Tema de AeTint (parcial)"),
                (_, true) => i18n::t("Themed by another app", "Con tema de otro programa"),
                _ => i18n::t("Original", "Original"),
            };
            out.push((t.id.clone(), if ver.is_empty() { s.to_string() } else { format!("{s} · v{ver}") }));
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
    ui.set_busy(true);
    ui.set_status_error(false);
    ui.set_status_ok(false);
    ui.set_status(if restore { t("Restoring…", "Restaurando…") } else { t("Applying…", "Aplicando…") }.into());
    let weak = ui.as_weak();
    let all = app.borrow().targets.clone();
    std::thread::spawn(move || {
        let r = if restore { install::restore(&targets) } else { install::apply(&targets, &cfg) };
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
        let r = menus::dll_path().ok_or("aetint_menu.dll is missing next to the .exe".to_string()).and_then(|d| menus::inject(pid, &d));
        println!("{r:?}");
        std::process::exit(if r.is_ok() { 0 } else { 1 });
    }
    // modo prueba: fabrica los archivos con tema en una carpeta, sin instalar nada
    if let Some(i) = args.iter().position(|a| a == "--dry-run") {
        let out = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "dry-run".into()));
        let cfg = install::load_config();
        let st = install::State::default();
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
    }));
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));

    ui.set_presets(preset_items(false));
    ui.set_themes(theme_items(&app.borrow().themes, false));
    menus::deploy_dll();

    {
        let g = ui.global::<Ui>();
        g.on_hsv_color(|h, s, v| slint_color(hsv_to_rgb(h, s, v)));
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
                let _ = menus::write_colors(&a.cfg.palette, &a.cfg.light, true);
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
            let need_watcher = on && !install::all_have_plugin(&app.borrow().targets);
            let r = menus::write_colors(&pal, &light, on).and_then(|_| if need_watcher { menus::start_watcher() } else { Ok(()) });
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
        // esquinas redondeadas de Windows 11 en la ventana sin marco
        let weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(50), move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w: &winit::window::Window| {
                    round_corners(w);
                });
            }
        });
    }
    // ── brillo que se edita: Dark (Darkest y Dark de AE) o Light
    ui.on_set_edit_light({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |light| {
            app.borrow_mut().edit_light = light;
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
    ui.set_autostart(menus::autostart_enabled());
    if app.borrow().cfg.menus {
        let _ = menus::write_colors(&app.borrow().cfg.palette, &app.borrow().cfg.light, true);
        if !install::all_have_plugin(&app.borrow().targets) {
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
    center_window(ui.window(), 1180.0, 760.0);
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
