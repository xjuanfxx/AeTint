//! Comprobación de actualizaciones: la última Release publicada en GitHub. Una sola petición al abrir
//! AeTint, en segundo plano; no se envía ningún dato del usuario. Sin conexión, simplemente no hay aviso.

pub const REPO: &str = "xjuanfxx/AeTint";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Una versión nueva disponible: número (sin la "v") y página de la Release.
pub struct Update {
    pub version: String,
    pub url: String,
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
        return is_newer(&v, CURRENT).then(|| Update { version: v.trim_start_matches('v').to_string(), url: format!("https://github.com/{REPO}/releases/latest") });
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
    is_newer(tag, CURRENT).then(|| Update { version: tag.trim_start_matches(['v', 'V']).to_string(), url })
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
