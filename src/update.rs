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
    let resp = http::get(&format!("https://api.github.com/repos/{REPO}/releases/latest"), &[("Accept", "application/vnd.github+json")], 8_000).ok()?;
    let v: serde_json::Value = serde_json::from_str(&resp.text().ok()?).ok()?;
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
    let get = |u: &str| http::get(u, &[], 120_000);
    let expected = get(sha)?.text()?;
    let expected = expected.split_whitespace().next().unwrap_or("").to_lowercase();
    if expected.len() != 64 {
        return Err(tr!("invalid checksum file", "archivo de comprobación no válido"));
    }
    let mut reader = get(setup)?;
    let total: Option<usize> = reader.len;
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

/// GET por HTTPS con WinHTTP, el cliente de Windows (certificados y proxy del sistema): así el .exe no
/// lleva su propia pila TLS. Sigue las redirecciones (las descargas de GitHub redirigen a su CDN).
#[cfg(windows)]
mod http {
    use windows_sys::Win32::Networking::WinHttp::*;
    type H = *mut core::ffi::c_void;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub struct Response {
        handles: [H; 3],
        /// Content-Length, si el servidor lo da
        pub len: Option<usize>,
    }

    impl Drop for Response {
        fn drop(&mut self) {
            for h in self.handles.iter().rev() {
                if !h.is_null() {
                    unsafe { WinHttpCloseHandle(*h) };
                }
            }
        }
    }

    impl std::io::Read for Response {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut n = 0u32;
            let ok = unsafe { WinHttpReadData(self.handles[2], buf.as_mut_ptr() as _, buf.len().min(u32::MAX as usize) as u32, &mut n) };
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(n as usize)
        }
    }

    impl Response {
        pub fn text(mut self) -> Result<String, String> {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut self, &mut s).map_err(|e| e.to_string())?;
            Ok(s)
        }
    }

    fn query_number(req: H, info: u32) -> Option<u32> {
        let (mut v, mut size) = (0u32, 4u32);
        let ok = unsafe { WinHttpQueryHeaders(req, info | WINHTTP_QUERY_FLAG_NUMBER, std::ptr::null(), &mut v as *mut u32 as _, &mut size, std::ptr::null_mut()) };
        (ok != 0).then_some(v)
    }

    pub fn get(url: &str, headers: &[(&str, &str)], timeout_ms: i32) -> Result<Response, String> {
        let rest = url.strip_prefix("https://").ok_or("only https")?;
        let (host, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((rest, "/".into()));
        let err = |what: &str| format!("{what}: {}", std::io::Error::last_os_error());
        let mut r = Response { handles: [std::ptr::null_mut(); 3], len: None };
        unsafe {
            let agent = wide(&format!("AeTint/{}", super::CURRENT));
            r.handles[0] = WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, std::ptr::null(), std::ptr::null(), 0);
            if r.handles[0].is_null() {
                return Err(err("WinHttpOpen"));
            }
            WinHttpSetTimeouts(r.handles[0], timeout_ms, timeout_ms, timeout_ms, timeout_ms);
            r.handles[1] = WinHttpConnect(r.handles[0], wide(host).as_ptr(), 443, 0);
            if r.handles[1].is_null() {
                return Err(err("WinHttpConnect"));
            }
            r.handles[2] = WinHttpOpenRequest(r.handles[1], wide("GET").as_ptr(), wide(&path).as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), WINHTTP_FLAG_SECURE);
            if r.handles[2].is_null() {
                return Err(err("WinHttpOpenRequest"));
            }
            let req = r.handles[2];
            let extra: String = headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect();
            if !extra.is_empty() {
                let w = wide(&extra);
                WinHttpAddRequestHeaders(req, w.as_ptr(), u32::MAX, WINHTTP_ADDREQ_FLAG_ADD);
            }
            if WinHttpSendRequest(req, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) == 0 {
                return Err(err("WinHttpSendRequest"));
            }
            if WinHttpReceiveResponse(req, std::ptr::null_mut()) == 0 {
                return Err(err("WinHttpReceiveResponse"));
            }
            let status = query_number(req, WINHTTP_QUERY_STATUS_CODE).unwrap_or(0);
            if !(200..300).contains(&status) {
                return Err(format!("HTTP {status}"));
            }
            r.len = query_number(req, WINHTTP_QUERY_CONTENT_LENGTH).map(|n| n as usize);
        }
        Ok(r)
    }
}

#[cfg(not(windows))]
mod http {
    pub struct Response {
        pub len: Option<usize>,
    }
    impl std::io::Read for Response {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }
    impl Response {
        pub fn text(self) -> Result<String, String> {
            Err("unsupported".into())
        }
    }
    pub fn get(_: &str, _: &[(&str, &str)], _: i32) -> Result<Response, String> {
        Err("unsupported".into())
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
