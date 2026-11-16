//! Instalaciones de After Effects, copias de los originales, escritura (con permisos de administrador si hace falta)
//! y restauración.
//!
//! Regla de oro: un tema siempre se fabrica a partir del ORIGINAL guardado en %LOCALAPPDATA%\AeTint\originals,
//! nunca encima de un archivo ya tematizado. Cada archivo escrito se verifica byte a byte.

use crate::aelib::AeLib;
use crate::dvaui::{Dvaui, Kind};
use crate::remap::Palette;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Target {
    pub id: String,
    pub label: String,
    pub support: PathBuf,
}

impl Target {
    pub fn file(&self, f: FileKind) -> PathBuf {
        self.support.join(f.name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Dvaui,
    AeLib,
    /// Un archivo de estilos de la pantalla de inicio (índice en `uxp::HOME_FILES`).
    Home(u8),
}

impl FileKind {
    pub fn name(self) -> &'static str {
        match self {
            FileKind::Dvaui => "dvaui.dll",
            FileKind::AeLib => "AfterFXLib.dll",
            FileKind::Home(i) => crate::uxp::HOME_FILES[i as usize],
        }
    }

    /// Todos los archivos que AeTint puede tocar en una instalación.
    pub fn all() -> Vec<FileKind> {
        let mut v = vec![FileKind::Dvaui, FileKind::AeLib];
        v.extend((0..crate::uxp::HOME_FILES.len()).map(|i| FileKind::Home(i as u8)));
        v
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    /// "<id>/<archivo>" → sha256 de lo último que escribió AeTint.
    pub written: BTreeMap<String, String>,
    /// Tema instalado en cada versión (para que AeTint abra con él).
    #[serde(default)]
    pub applied: BTreeMap<String, AppliedTheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedTheme {
    pub palette: Palette,
    #[serde(default)]
    pub light: Option<Palette>,
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
    /// tamaño y fecha del dvaui.dll escrito: comprobación rápida de que sigue siendo el nuestro
    pub size: u64,
    pub mtime: u64,
}

fn file_stamp(p: &Path) -> Option<(u64, u64)> {
    let m = fs::metadata(p).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((m.len(), t))
}

/// Qué tema tiene instalada una versión de AE, sin leer los archivos enteros.
pub enum Installed {
    /// el tema de AeTint, con esta paleta
    Ours(AppliedTheme),
    /// los archivos de Adobe sin tocar
    Original,
    /// otro programa o no se sabe
    Unknown,
}

pub fn installed_theme(t: &Target) -> Installed {
    let st = load_state();
    let dv = t.file(FileKind::Dvaui);
    if let (Some(a), Some((size, mtime))) = (st.applied.get(&t.id), file_stamp(&dv)) {
        if a.size == size && a.mtime == mtime {
            return Installed::Ours(a.clone());
        }
    }
    match fs::read(&dv) {
        Ok(b) if st.written.get(&key(t, FileKind::Dvaui)) != Some(&sha(&b)) && Dvaui::open(&b).is_ok() => Installed::Original,
        _ => Installed::Unknown,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Paleta oscura: brillos Darkest y Dark de AE.
    pub palette: Palette,
    /// Paleta clara: brillo Light de AE.
    #[serde(default = "Palette::light_default")]
    pub light: Palette,
    /// Colores de AE puestos a mano (claves sin "ae-color-").
    pub overrides: BTreeMap<String, String>,
    pub selected: Vec<String>,
    /// Menús Win32 y barra de título (aetint_menu.dll).
    #[serde(default)]
    pub menus: bool,
    /// Idioma de la interfaz: "en" (por defecto) o "es".
    #[serde(default = "default_lang")]
    pub lang: String,
}

fn default_lang() -> String {
    "en".into()
}

impl Default for Config {
    fn default() -> Self {
        Config { palette: Palette::default(), light: Palette::light_default(), overrides: BTreeMap::new(), selected: Vec::new(), menus: false, lang: default_lang() }
    }
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("AeTint")
}

fn load<T: for<'a> Deserialize<'a> + Default>(name: &str) -> T {
    fs::read(data_dir().join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn save<T: Serialize>(name: &str, v: &T) {
    let _ = fs::create_dir_all(data_dir());
    if let Ok(s) = serde_json::to_vec_pretty(v) {
        let _ = fs::write(data_dir().join(name), s);
    }
}
/// Un tema guardado por el usuario en la biblioteca ("My themes").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTheme {
    pub name: String,
    pub palette: Palette,
    #[serde(default)]
    pub light: Option<Palette>,
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
}

pub fn load_themes() -> Vec<SavedTheme> {
    load("themes.json")
}
pub fn save_themes(t: &[SavedTheme]) {
    save("themes.json", &t.to_vec())
}

pub fn load_config() -> Config {
    load("config.json")
}
pub fn save_config(c: &Config) {
    save("config.json", c)
}
fn load_state() -> State {
    load("state.json")
}
fn save_state(s: &State) {
    save("state.json", s)
}

pub fn sha(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub fn find_targets() -> Vec<Target> {
    let mut roots: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles"].iter().filter_map(|v| std::env::var_os(v).map(PathBuf::from)).collect();
    roots.push(PathBuf::from(r"C:\Program Files"));
    let mut out: Vec<Target> = Vec::new();
    for root in roots {
        let Ok(rd) = fs::read_dir(root.join("Adobe")) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("Adobe After Effects") {
                continue;
            }
            let support = e.path().join("Support Files");
            if !support.join("dvaui.dll").is_file() || !support.join("AfterFXLib.dll").is_file() {
                continue;
            }
            let id = slug(&name);
            if out.iter().any(|t| t.id == id) {
                continue;
            }
            out.push(Target { id, label: name.trim_start_matches("Adobe ").to_string(), support });
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// Plugin de arranque que carga la DLL de los menús (crates/loader), instalado en "Plug-ins".
pub const AEX_NAME: &str = "AeTint.aex";

pub fn aex_path(t: &Target) -> PathBuf {
    t.support.join("Plug-ins").join(AEX_NAME)
}

/// ¿Esta versión de AE tiene el plugin de arranque, y es el de esta versión de AeTint?
pub fn plugin_ok(t: &Target) -> bool {
    matches!((fs::read(aex_path(t)), loader_bytes()), (Ok(a), Some(l)) if a == l)
}

/// Todas las versiones instaladas cargan los menús solas (sin vigilante).
pub fn all_have_plugin(targets: &[Target]) -> bool {
    targets.iter().all(plugin_ok)
}

fn loader_bytes() -> Option<Vec<u8>> {
    fs::read(std::env::current_exe().ok()?.with_file_name("aetint_loader.dll")).ok()
}

fn backup_path(t: &Target, f: FileKind) -> PathBuf {
    data_dir().join("originals").join(&t.id).join(f.name())
}

fn key(t: &Target, f: FileKind) -> String {
    format!("{}/{}", t.id, f.name())
}

/// ¿Este contenido sirve como original?
fn is_pristine(f: FileKind, b: &[u8]) -> bool {
    match f {
        FileKind::Dvaui => Dvaui::open(b).is_ok(),
        FileKind::AeLib => AeLib::open(b).is_ok_and(|a| a.looks_original(b)),
        // estilos de la Home: si no es lo último que escribió AeTint, es el de Adobe
        FileKind::Home(_) => true,
    }
}

/// El original de un archivo: la copia guardada si lo instalado es un tema nuestro; si no, lo instalado
/// (y se guarda como copia). Si lo instalado tiene un tema de otro programa, se busca su original.
pub fn original(t: &Target, f: FileKind, st: &State) -> Result<Vec<u8>, String> {
    let path = t.file(f);
    let cur = fs::read(&path).map_err(|e| tr!("couldn't read {}: {}", "no se pudo leer {}: {}", path.display(), e))?;
    let bk = backup_path(t, f);
    let cur_sha = sha(&cur);
    if st.written.get(&key(t, f)) == Some(&cur_sha) {
        if let Ok(b) = fs::read(&bk) {
            return Ok(b);
        }
        return Err(tr!("{} has an AeTint theme but its original backup is missing", "{} tiene un tema de AeTint pero falta la copia original", f.name()));
    }
    // Home: si lo instalado es la copia guardada con otros colores, es un tema (nuestro o de otro programa):
    // se conserva la copia. Solo una actualización de Adobe (otro contenido) cuenta como original nuevo.
    if let FileKind::Home(_) = f {
        if let Ok(b) = fs::read(&bk) {
            if crate::uxp::same_but_colors(&cur, &b) {
                return Ok(b);
            }
        }
    }
    if is_pristine(f, &cur) {
        if fs::read(&bk).map(|b| sha(&b) != cur_sha).unwrap_or(true) {
            fs::create_dir_all(bk.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::write(&bk, &cur).map_err(|e| tr!("couldn't save the original backup: {}", "no se pudo guardar la copia original: {}", e))?;
        }
        return Ok(cur);
    }
    // Tema de otro programa: nuestra copia, o el original que guardó Nothi Theme.
    let mut cands = vec![bk.clone()];
    if let Some(la) = std::env::var_os("LOCALAPPDATA") {
        cands.push(PathBuf::from(la).join("NothiTheme").join("Originals").join(&t.id).join(f.name()));
    }
    for c in cands {
        if let Ok(b) = fs::read(&c) {
            if b.len() == cur.len() && is_pristine(f, &b) {
                if c != bk {
                    fs::create_dir_all(bk.parent().unwrap()).map_err(|e| e.to_string())?;
                    fs::write(&bk, &b).map_err(|e| e.to_string())?;
                }
                return Ok(b);
            }
        }
    }
    if f == FileKind::AeLib && AeLib::open(&cur).is_ok() {
        // Colores de AE ya cambiados por otra herramienta y sin original a mano: se parte de lo que hay.
        fs::create_dir_all(bk.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&bk, &cur).map_err(|e| e.to_string())?;
        return Ok(cur);
    }
    Err(format!(
        "{} de {} ya tiene un tema de otro programa y no hay original guardado. Restáuralo con ese programa primero.",
        f.name(),
        t.label
    ))
}

pub fn ae_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq AfterFX.exe", "/NH", "/FO", "CSV"])
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(o) = out {
            return String::from_utf8_lossy(&o.stdout).to_lowercase().contains("afterfx.exe");
        }
    }
    false
}

/// Desarrollo: abre los DLL de una carpeta y prueba a aplicarles la paleta, contando qué encuentra.
pub fn probe(dir: &Path, cfg: &Config) {
    if std::env::var_os("AETINT_PROBE_RES").is_some() {
        for f in ["dvaui.dll", "AfterFXLib.dll"] {
            if let Ok(b) = std::fs::read(dir.join(f)) {
                if let Ok(p) = crate::pe::Pe::parse(&b) {
                    for r in p.resources(&b) {
                        let t = r.path.first().map(|k| k.to_string()).unwrap_or_default();
                        if !matches!(t.as_str(), "JSON" | "CSS" | "XML") { continue; }
                        let path: Vec<String> = r.path.iter().map(|k| k.to_string()).collect();
                        println!("  {f} res {} {} bytes @{:#x}", path.join("/"), r.size, r.off);
                    }
                }
            }
        }
    }
    let (Ok(pal), Ok(lpal)) = (cfg.palette.resolve(), cfg.light.resolve()) else { return };
    match std::fs::read(dir.join("dvaui.dll")) {
        Err(e) => println!("  dvaui.dll: {e}"),
        Ok(b) => match Dvaui::open(&b) {
            Err(e) => println!("  dvaui.dll: {e}"),
            Ok(d) => {
                let count = |k: Kind| d.tables.iter().filter(|t| t.kind == k).count();
                for k in [Kind::Darkest, Kind::Dark, Kind::Light] {
                    if let Some(t) = d.tone_for(&b, k) {
                        println!("  tone {k:?}: base {:.0} raised {:.0} panel {:.0} frame {:?}", t.base * 255.0, t.raised * 255.0, t.panel * 255.0, t.frame.map(|f| (f * 255.0).round()));
                    }
                }
                println!("  legacy {}", d.legacy.is_some());
                println!("  dvaui.dll {}: {} tokens, tables darkest {} dark {} light {}, dna {}, css {}", d.version, d.names.len(), count(Kind::Darkest), count(Kind::Dark), count(Kind::Light), d.dna.is_some(), d.css.len());
                match d.apply(&b, &pal, Some(&lpal)) {
                    Ok(n) => { if let Some(o) = std::env::var_os("AETINT_PROBE_OUT") { let _ = std::fs::write(Path::new(&o).join(format!("dvaui-{}.dll", d.version)), &n); } println!("  dvaui apply ok, {} bytes changed, reopen: {:?}", n.iter().zip(&b).filter(|(x, y)| x != y).count(), Dvaui::open(&n).err()) }
                    Err(e) => println!("  dvaui apply: {e}"),
                }
                match std::fs::read(dir.join("AfterFXLib.dll")) {
                    Err(e) => println!("  AfterFXLib.dll: {e}"),
                    Ok(a) => match AeLib::open(&a) {
                        Err(e) => println!("  AfterFXLib.dll: {e}"),
                        Ok(l) => {
                            let blocks = l.json(&a).map(|v| v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default());
                            println!("  AfterFXLib.dll {}: blocks {:?}, original {}", l.version, blocks, l.looks_original(&a));
                            let mut bl = Vec::new();
                            for (name, k) in [("darkest", Kind::Darkest), ("dark", Kind::Dark)] {
                                if let Some(t) = d.tone_for(&b, k) {
                                    bl.push((name, t, &pal));
                                }
                            }
                            if let Some(t) = d.tone_for(&b, Kind::Light) {
                                bl.push(("light", t, &lpal));
                            }
                            match l.apply(&a, &bl, &cfg.overrides) {
                                Ok(n) => println!("  aelib apply ok, {} bytes changed", n.iter().zip(&a).filter(|(x, y)| x != y).count()),
                                Err(e) => println!("  aelib apply: {e}"),
                            }
                        }
                    },
                }
            }
        },
    }
}

/// Fabrica los archivos con tema de una instalación (sin escribir nada todavía).
pub fn build(t: &Target, cfg: &Config, st: &State) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let pal = cfg.palette.resolve()?;
    let lpal = cfg.light.resolve()?;
    let od = original(t, FileKind::Dvaui, st)?;
    let d = Dvaui::open(&od)?;
    let nd = d.apply(&od, &pal, Some(&lpal))?;
    let mut tones = Vec::new();
    if let Some(x) = d.tone_for(&od, Kind::Darkest) {
        tones.push(("darkest", x));
    }
    if let Some(x) = d.tone_for(&od, Kind::Dark) {
        tones.push(("dark", x));
    }
    let light_tone = d.tone_for(&od, Kind::Light);
    let oa = original(t, FileKind::AeLib, st)?;
    let a = AeLib::open(&oa)?;
    let mut blocks: Vec<(&str, crate::remap::ToneRef, &crate::remap::Pal)> = tones.iter().map(|(b, x)| (*b, *x, &pal)).collect();
    if let Some(lt) = light_tone {
        blocks.push(("light", lt, &lpal));
    }
    let na = a.apply(&oa, &blocks, &cfg.overrides)?;
    let mut out = vec![(t.file(FileKind::Dvaui), nd), (t.file(FileKind::AeLib), na)];
    // pantalla de inicio (opcional: si esta versión no la tiene o algo falla, se deja como está)
    let darkest = tones.iter().find(|x| x.0 == "darkest").map(|x| x.1);
    let dark = tones.iter().find(|x| x.0 == "dark").map(|x| x.1).or(darkest);
    if let (Some(td), Some(tk)) = (darkest, dark) {
        for f in FileKind::all().into_iter().filter(|f| matches!(f, FileKind::Home(_))) {
            let path = t.file(f);
            if !path.is_file() {
                continue;
            }
            // si algo falla aquí no se salta en silencio: el error dice qué archivo y por qué
            let o = original(t, f, st).map_err(|e| tr!("Home screen: {}", "Pantalla de inicio: {}", e))?;
            let new = crate::uxp::theme_file(&o, &pal, &td, &tk, light_tone.as_ref().map(|lt| (&lpal, lt))).unwrap_or(o);
            // si no se puede leer lo instalado (en uso), se intenta escribir igual y la escritura lo explica
            if fs::read(&path).map(|cur| sha(&cur) != sha(&new)).unwrap_or(true) {
                out.push((path, new));
            }
        }
    }
    Ok(out)
}

pub fn apply(targets: &[Target], cfg: &Config) -> Result<String, String> {
    if ae_running() {
        return Err(crate::i18n::t("Close After Effects before applying the theme.", "Cierra After Effects antes de aplicar el tema.").into());
    }
    let mut st = load_state();
    let mut plan = Vec::new();
    for t in targets {
        for (dst, bytes) in build(t, cfg, &st).map_err(|e| format!("{}: {e}", t.label))? {
            plan.push((dst, bytes));
        }
        // con los menús activados, el plugin de arranque (sin vigilante ni inyección)
        if cfg.menus {
            if let Some(aex) = loader_bytes() {
                let dst = aex_path(t);
                if fs::read(&dst).map(|b| b != aex).unwrap_or(true) {
                    plan.push((dst, aex));
                }
            }
        }
    }
    if cfg.menus {
        crate::menus::deploy_dll();
    }
    write_all(&plan)?;
    for t in targets {
        for f in FileKind::all() {
            if let Ok(b) = fs::read(t.file(f)) {
                st.written.insert(key(t, f), sha(&b));
            }
        }
    }
    for t in targets {
        if let Some((size, mtime)) = file_stamp(&t.file(FileKind::Dvaui)) {
            st.applied.insert(t.id.clone(), AppliedTheme { palette: cfg.palette.clone(), light: Some(cfg.light.clone()), overrides: cfg.overrides.clone(), size, mtime });
        }
    }
    save_state(&st);
    let names: Vec<&str> = targets.iter().map(|t| t.label.as_str()).collect();
    Ok(tr!("Theme applied to {}.", "Tema aplicado en {}.", names.join(", ")))
}

pub fn restore(targets: &[Target]) -> Result<String, String> {
    if ae_running() {
        return Err(crate::i18n::t("Close After Effects before restoring.", "Cierra After Effects antes de restaurar.").into());
    }
    let mut st = load_state();
    let mut plan = Vec::new();
    for t in targets {
        for f in FileKind::all() {
            let bk = backup_path(t, f);
            let Ok(b) = fs::read(&bk) else { continue };
            let cur = fs::read(t.file(f)).unwrap_or_default();
            if sha(&cur) != sha(&b) {
                plan.push((t.file(f), b));
            }
        }
    }
    if !plan.is_empty() {
        write_all(&plan)?;
    }
    let aexs: Vec<PathBuf> = targets.iter().map(aex_path).filter(|p| p.is_file()).collect();
    if !aexs.is_empty() {
        remove_all(&aexs)?;
    }
    for t in targets {
        for f in FileKind::all() {
            st.written.remove(&key(t, f));
        }
        st.applied.remove(&t.id);
    }
    save_state(&st);
    Ok(if plan.is_empty() && aexs.is_empty() { crate::i18n::t("The originals were already in place.", "Ya estaban los originales.") } else { crate::i18n::t("Originals restored.", "Originales restaurados.") }.into())
}

// ── escritura

fn can_write(p: &Path) -> Result<bool, String> {
    if !p.exists() {
        // archivo nuevo: se intenta crear aquí; si no hay permiso, lo hace la copia con permisos de administrador
        return match fs::OpenOptions::new().write(true).create_new(true).open(p) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
            Err(e) => Err(format!("{}: {e}", p.display())),
        };
    }
    match fs::OpenOptions::new().write(true).open(p) {
        Ok(_) => Ok(true),
        Err(e) if e.raw_os_error() == Some(32) || e.raw_os_error() == Some(33) => {
            Err(tr!("{} is in use (is After Effects, Media Encoder or aerender open?)", "{} está en uso (¿After Effects, Media Encoder o aerender abiertos?)", p.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

fn write_in_place(dst: &Path, b: &[u8]) -> std::io::Result<()> {
    // se escribe encima (no se reemplaza el archivo) para conservar sus permisos
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).open(dst)?;
    f.write_all(b)?;
    f.sync_all()
}

fn verify(plan: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    for (dst, b) in plan {
        let got = fs::read(dst).map_err(|e| e.to_string())?;
        if sha(&got) != sha(b) {
            return Err(tr!("{} was not written as expected", "{} no quedó como se esperaba", dst.display()));
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    items: Vec<(PathBuf, PathBuf)>,
    user: String,
    /// archivos que hay que borrar (el plugin al restaurar)
    #[serde(default)]
    delete: Vec<PathBuf>,
}

fn write_all(plan: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    let mut direct = true;
    for (dst, _) in plan {
        if !can_write(dst)? {
            direct = false;
        }
    }
    if direct {
        for (dst, b) in plan {
            write_in_place(dst, b).map_err(|e| format!("{}: {e}", dst.display()))?;
        }
        return verify(plan);
    }
    // Sin permiso: se dejan los archivos en staging y una copia de AeTint con permisos de administrador los pone
    // en su sitio y da permiso de escritura a este usuario sobre esos archivos (las próximas veces no pregunta).
    let stage = data_dir().join("staging");
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    for (i, (dst, b)) in plan.iter().enumerate() {
        let src = stage.join(format!("{i}.bin"));
        fs::write(&src, b).map_err(|e| e.to_string())?;
        items.push((src, dst.clone()));
    }
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) => format!("{d}\\{u}"),
        (_, Ok(u)) => u,
        _ => String::new(),
    };
    let man = stage.join("manifest.json");
    fs::write(&man, serde_json::to_vec(&Manifest { items, user, delete: Vec::new() }).unwrap()).map_err(|e| e.to_string())?;
    run_elevated(&man)?;
    let r = verify(plan);
    let _ = fs::remove_dir_all(&stage);
    r
}

/// Borra archivos de Program Files (el plugin al restaurar), con permisos de administrador si hace falta.
fn remove_all(paths: &[PathBuf]) -> Result<(), String> {
    let mut left = Vec::new();
    for p in paths {
        if fs::remove_file(p).is_err() {
            left.push(p.clone());
        }
    }
    if left.is_empty() {
        return Ok(());
    }
    let stage = data_dir().join("staging");
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let man = stage.join("manifest.json");
    fs::write(&man, serde_json::to_vec(&Manifest { items: Vec::new(), user: String::new(), delete: left.clone() }).unwrap()).map_err(|e| e.to_string())?;
    run_elevated(&man)?;
    let _ = fs::remove_dir_all(&stage);
    if left.iter().any(|p| p.exists()) {
        return Err(tr!("Couldn't remove {}. Is After Effects still open?", "No se pudo quitar {}. ¿After Effects sigue abierto?", AEX_NAME));
    }
    Ok(())
}

/// Modo `--install <manifest>` (proceso con permisos de administrador).
pub fn run_install(manifest: &Path) -> i32 {
    let Ok(b) = fs::read(manifest) else { return 2 };
    let Ok(m) = serde_json::from_slice::<Manifest>(&b) else { return 3 };
    for p in &m.delete {
        if fs::remove_file(p).is_err() && p.exists() {
            return 6;
        }
    }
    for (src, dst) in &m.items {
        let Ok(data) = fs::read(src) else { return 4 };
        if write_in_place(dst, &data).is_err() {
            return 5;
        }
        if !m.user.is_empty() {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let _ = std::process::Command::new("icacls")
                    .arg(dst)
                    .args(["/grant", &format!("{}:(M)", m.user)])
                    .creation_flags(0x0800_0000)
                    .status();
            }
        }
    }
    0
}

#[cfg(windows)]
fn run_elevated(manifest: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let w = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_w = w(exe.as_os_str());
    let args_w = w(std::ffi::OsStr::new(&format!("--install \"{}\"", manifest.display())));
    let verb = w(std::ffi::OsStr::new("runas"));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe_w.as_ptr();
        info.lpParameters = args_w.as_ptr();
        info.nShow = SW_HIDE;
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return Err(crate::i18n::t("Administrator permission is needed to write to Program Files (cancelled).", "Hace falta permiso de administrador para escribir en Program Files (cancelado).").into());
        }
        let ok = WaitForSingleObject(info.hProcess, INFINITE) == WAIT_OBJECT_0;
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if !ok || code != 0 {
            return Err(tr!("Couldn't write the files (code {}). Is After Effects still open?", "No se pudieron escribir los archivos (código {}). ¿After Effects sigue abierto?", code));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn run_elevated(_: &Path) -> Result<(), String> {
    Err(crate::i18n::t("no write permission", "sin permiso de escritura").into())
}
