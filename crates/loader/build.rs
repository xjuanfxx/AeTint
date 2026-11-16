//! Recurso PiPL: la "tarjeta" que After Effects lee de cada plugin para saber qué es y cómo llamarlo.
//! Aquí: un plugin general (AEGP) llamado AeTint cuya función de entrada es `EntryPointFunc`.
//!
//! Formato en Windows: WORD 1, LONG versión 0, LONG nº de propiedades y, por cada una:
//! proveedor '8BIM', clave, LONG id 0, LONG longitud y los datos rellenados a múltiplos de 4.
//! Los códigos de 4 letras van como enteros little-endian (por eso '8BIM' queda "MIB8").

use std::io::Write;

fn four(code: &[u8; 4]) -> [u8; 4] {
    u32::from_be_bytes(*code).to_le_bytes()
}

fn prop(out: &mut Vec<u8>, key: &[u8; 4], data: &[u8]) {
    let mut d = data.to_vec();
    while d.len() % 4 != 0 {
        d.push(0);
    }
    out.extend_from_slice(&four(b"8BIM"));
    out.extend_from_slice(&four(key));
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(d.len() as u32).to_le_bytes());
    out.extend_from_slice(&d);
}

fn pascal(s: &str) -> Vec<u8> {
    let mut v = vec![s.len() as u8];
    v.extend_from_slice(s.as_bytes());
    v
}

fn main() {
    let mut props = Vec::new();
    prop(&mut props, b"kind", &four(b"AEgp"));
    prop(&mut props, b"name", &pascal("AeTint"));
    prop(&mut props, b"catg", &pascal("General Plugin"));
    // versión del plugin (3.0, como los ejemplos de AEGP del SDK de After Effects)
    prop(&mut props, b"vers", &196608u32.to_le_bytes());
    prop(&mut props, b"8664", b"EntryPointFunc\0");
    // banderas de AE (las llevan todos los plugins de Adobe, como EasyEase.aex)
    prop(&mut props, b"aeFL", &0u32.to_le_bytes());

    let mut pipl = Vec::new();
    pipl.extend_from_slice(&1u16.to_le_bytes());
    pipl.extend_from_slice(&0u32.to_le_bytes());
    pipl.extend_from_slice(&6u32.to_le_bytes());
    pipl.extend_from_slice(&props);

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let bin = out.join("pipl.bin");
    std::fs::File::create(&bin).unwrap().write_all(&pipl).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set("FileDescription", "AeTint loader for After Effects");
        res.set("ProductName", "AeTint");
        let path = bin.display().to_string().replace('\\', "\\\\");
        res.append_rc_content(&format!("16000 PiPL \"{path}\"\n"));
        res.compile().expect("no se pudo compilar el recurso PiPL");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
