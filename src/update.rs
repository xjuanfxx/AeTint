//! Comprobación de actualizaciones: la última Release publicada en GitHub. Una sola petición al abrir
//! AeTint, en segundo plano; no se envía ningún dato del usuario. Sin conexión, simplemente no hay aviso.

pub const REPO: &str = "xjuanfxx/AeTint";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Una versión nueva disponible: número (sin la "v"), página de la Release y, si están publicados,
/// el instalador y su huella SHA-256.
#[derive(Clone)]
pub struct Update {
    pub version: String,
    pub url: String,
    pub setup_url: Option<String>,
    pub sha_url: Option<String>,
}

fn parse(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map(|p| p.parse::<u32>());
    Some((it.next()?.ok()?, it.next().and_then(Result::ok).unwrap_or(0), it.next().and_then(Result::ok).unwrap_or(0)))
}

/// ¿`latest` es más nueva que `current`?
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

/// Consulta la última Release. `AETINT_FAKE_LATEST` (por ejemplo "9.9.9") simula una para probar el aviso.
pub fn check() -> Option<Update> {
    if let Ok(v) = std::env::var("AETINT_FAKE_LATEST") {
        return is_newer(&v, CURRENT).then(|| Update { version: v.trim_start_matches('v').to_string(), url: format!("https://github.com/{REPO}/releases/latest"), setup_url: None, sha_url: None });
    }
    let resp = ureq::get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("User-Agent", &format!("AeTint/{CURRENT}"))
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .ok()?;
    let v: serde_json::Value = resp.into_json().ok()?;
    let tag = v["tag_name"].as_str()?;
    let url = v["html_url"].as_str().unwrap_or("").to_string();
    let asset = |name: &str| {
        v["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name)).and_then(|a| a["browser_download_url"].as_str()).map(String::from)
    };
    is_newer(tag, CURRENT).then(|| Update {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url,
        setup_url: asset("AeTint-Setup.exe"),
        sha_url: asset("AeTint-Setup.exe.sha256"),
    })
}

/// Descarga el instalador de la actualización, comprueba su huella SHA-256 y devuelve dónde quedó.
/// Solo acepta direcciones de GitHub por HTTPS. `progress` recibe la fracción descargada (0..1).
pub fn download(up: &Update, progress: impl Fn(f32)) -> Result<std::path::PathBuf, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let (Some(setup), Some(sha)) = (&up.setup_url, &up.sha_url) else {
        return Err(crate::i18n::t("This release has no installer to update automatically.", "Esta versión no tiene instalador para actualizar automáticamente.").into());
    };
    let github = |u: &str| u.starts_with("https://github.com/") || u.starts_with("https://objects.githubusercontent.com/");
    if !github(setup) || !github(sha) {
        return Err(tr!("unexpected download address", "dirección de descarga inesperada"));
    }
    let agent = format!("AeTint/{CURRENT}");
    let get = |u: &str| ureq::get(u).set("User-Agent", &agent).timeout(std::time::Duration::from_secs(120)).call().map_err(|e| e.to_string());
    let expected = get(sha)?.into_string().map_err(|e| e.to_string())?;
    let expected = expected.split_whitespace().next().unwrap_or("").to_lowercase();
    if expected.len() != 64 {
        return Err(tr!("invalid checksum file", "archivo de comprobación no válido"));
    }
    let resp = get(setup)?;
    let total: Option<usize> = resp.header("Content-Length").and_then(|v| v.parse().ok());
    let mut reader = resp.into_reader();
    let mut data = Vec::with_capacity(total.unwrap_or(0));
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(t) = total {
            progress((data.len() as f32 / t as f32).min(1.0));
        }
    }
    let got: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    if got != expected {
        return Err(crate::i18n::t("The downloaded installer doesn't match its checksum; it wasn't run.", "El instalador descargado no coincide con su huella; no se ha ejecutado.").into());
    }
    let path = std::env::temp_dir().join(format!("AeTint-Setup-{}.exe", up.version));
    std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Lanza el instalador descargado en modo actualización (se abre mostrando el progreso, cierra AeTint,
/// sustituye los archivos y vuelve a abrir AeTint).
pub fn run_installer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(path).arg("--update").spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Abre la página de la Release en el navegador.
pub fn open(url: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        if url.starts_with("https://") {
            ShellExecuteW(std::ptr::null_mut(), w("open").as_ptr(), w(url).as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;
    #[test]
    fn versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }
}

#[cfg(test)]
mod real {
    /// Descarga real del instalador de una Release publicada y comprobación de su huella (no lo ejecuta).
    /// `AETINT_TEST_TAG=v0.2.3 cargo test real_download -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_download() {
        let tag = std::env::var("AETINT_TEST_TAG").unwrap_or_else(|_| "v0.2.3".into());
        let base = format!("https://github.com/{}/releases/download/{tag}", super::REPO);
        let up = super::Update {
            version: tag.trim_start_matches('v').into(),
            url: String::new(),
            setup_url: Some(format!("{base}/AeTint-Setup.exe")),
            sha_url: Some(format!("{base}/AeTint-Setup.exe.sha256")),
        };
        let p = super::download(&up, |_| {}).expect("descarga o huella");
        println!("descargado y verificado: {} ({} bytes)", p.display(), std::fs::metadata(&p).unwrap().len());
        let _ = std::fs::remove_file(p);
    }
}
