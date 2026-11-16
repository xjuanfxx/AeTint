//! El instalador lleva dentro los archivos de AeTint (AeTint.exe, aetint_menu.dll, aetint_loader.dll).
//! Se toman de `AETINT_PAYLOAD` o de `target/release`: hay que compilar AeTint antes que el instalador
//!   cargo build --release --workspace --exclude aetint_setup
//!   cargo build --release -p aetint_setup
//! Si faltan, el instalador se compila igual pero avisa de que no lleva los archivos.

use std::path::PathBuf;

pub const FILES: &[&str] = &["AeTint.exe", "aetint_menu.dll", "aetint_loader.dll"];

fn main() {
    slint_build::compile("ui/setup.slint").expect("Slint build failed");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = std::env::var_os("AETINT_PAYLOAD").map(PathBuf::from).unwrap_or_else(|| manifest.join("../../target/release"));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("payload");
    std::fs::create_dir_all(&out).unwrap();
    let mut missing = Vec::new();
    for f in FILES {
        let p = src.join(f);
        println!("cargo:rerun-if-changed={}", p.display());
        match std::fs::read(&p) {
            Ok(b) => std::fs::write(out.join(f), b).unwrap(),
            Err(_) => {
                std::fs::write(out.join(f), b"").unwrap();
                missing.push(*f);
            }
        }
    }
    if !missing.is_empty() {
        println!("cargo:warning=el instalador no lleva {}: compila AeTint antes (cargo build --release --workspace --exclude aetint_setup)", missing.join(", "));
    }
    println!("cargo:rerun-if-env-changed=AETINT_PAYLOAD");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.set("FileDescription", "AeTint Setup");
        res.set("ProductName", "AeTint");
        let _ = res.compile();
    }
}
