//! Lectura mínima de un PE de Windows x64: secciones, recursos, versión y checksum.
//! Solo lo necesario para localizar tablas de color y recursos de texto dentro de las DLL de Adobe.

use std::fmt;

#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub vsize: u32,
    pub rva: u32,
    pub raw_size: u32,
    pub raw: u32,
}

#[derive(Debug, Clone)]
pub struct Pe {
    pub image_base: u64,
    pub secs: Vec<Section>,
    res_rva: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResKey {
    Id(u32),
    Name(String),
}

impl fmt::Display for ResKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResKey::Id(n) => write!(f, "{n}"),
            ResKey::Name(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub path: Vec<ResKey>,
    /// Desplazamiento en el archivo (no RVA).
    pub off: usize,
    pub size: usize,
}

impl Resource {
    pub fn is(&self, ty: &str, name: &str) -> bool {
        matches!(self.path.first(), Some(ResKey::Name(t)) if t == ty)
            && matches!(self.path.get(1), Some(ResKey::Name(n)) if n == name)
    }
}

pub fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
pub fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
pub fn f32le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl Pe {
    pub fn parse(b: &[u8]) -> Result<Pe, String> {
        if b.len() < 0x400 || u16le(b, 0) != 0x5a4d {
            return Err(tr!("not a PE file", "no es un archivo PE"));
        }
        let e = u32le(b, 0x3c) as usize;
        if e + 0x200 > b.len() || u32le(b, e) != 0x4550 {
            return Err(tr!("not a PE file", "no es un archivo PE"));
        }
        let machine = u16le(b, e + 4);
        let nsec = u16le(b, e + 6) as usize;
        let opt_size = u16le(b, e + 20) as usize;
        let opt = e + 24;
        if u16le(b, opt) != 0x20b || machine != 0x8664 {
            return Err(tr!("not a 64-bit PE file", "no es un PE de 64 bits"));
        }
        let image_base = u64le(b, opt + 24);
        let dirs = opt + 112;
        let res_rva = u32le(b, dirs + 2 * 8);
        let mut secs = Vec::with_capacity(nsec);
        for i in 0..nsec {
            let s = opt + opt_size + i * 40;
            let name = String::from_utf8_lossy(&b[s..s + 8]).trim_end_matches('\0').to_string();
            secs.push(Section {
                name,
                vsize: u32le(b, s + 8),
                rva: u32le(b, s + 12),
                raw_size: u32le(b, s + 16),
                raw: u32le(b, s + 20),
            });
        }
        Ok(Pe { image_base, secs, res_rva })
    }

    pub fn sec(&self, name: &str) -> Option<&Section> {
        self.secs.iter().find(|s| s.name == name)
    }

    /// RVA → desplazamiento en el archivo.
    pub fn off(&self, rva: u32) -> Option<usize> {
        for s in &self.secs {
            let span = s.vsize.max(s.raw_size);
            if rva >= s.rva && rva < s.rva + span {
                let o = s.raw + (rva - s.rva);
                return (o < s.raw + s.raw_size).then_some(o as usize);
            }
        }
        None
    }

    /// Desplazamiento en el archivo → RVA.
    pub fn rva(&self, off: usize) -> Option<u32> {
        let off = off as u32;
        self.secs
            .iter()
            .find(|s| off >= s.raw && off < s.raw + s.raw_size)
            .map(|s| s.rva + off - s.raw)
    }

    /// Árbol de recursos aplanado: tipo → nombre → idioma.
    pub fn resources(&self, b: &[u8]) -> Vec<Resource> {
        let mut out = Vec::new();
        if self.res_rva == 0 {
            return out;
        }
        let Some(root) = self.off(self.res_rva) else { return out };
        self.res_dir(b, root, 0, 0, &mut Vec::new(), &mut out);
        out
    }

    fn res_dir(&self, b: &[u8], root: usize, o: usize, depth: usize, path: &mut Vec<ResKey>, out: &mut Vec<Resource>) {
        if depth > 3 || root + o + 16 > b.len() {
            return;
        }
        let n = u16le(b, root + o + 12) as usize + u16le(b, root + o + 14) as usize;
        for i in 0..n.min(8192) {
            let ent = root + o + 16 + i * 8;
            if ent + 8 > b.len() {
                return;
            }
            let nm = u32le(b, ent);
            let to = u32le(b, ent + 4);
            let key = if nm & 0x8000_0000 != 0 {
                let p = root + (nm & 0x7fff_ffff) as usize;
                let len = u16le(b, p) as usize;
                let units: Vec<u16> = (0..len).map(|k| u16le(b, p + 2 + k * 2)).collect();
                ResKey::Name(String::from_utf16_lossy(&units))
            } else {
                ResKey::Id(nm)
            };
            path.push(key);
            if to & 0x8000_0000 != 0 {
                self.res_dir(b, root, (to & 0x7fff_ffff) as usize, depth + 1, path, out);
            } else {
                let d = root + to as usize;
                let rva = u32le(b, d);
                let size = u32le(b, d + 4) as usize;
                if let Some(off) = self.off(rva) {
                    if off + size <= b.len() {
                        out.push(Resource { path: path.clone(), off, size });
                    }
                }
            }
            path.pop();
        }
    }
}

/// "FileVersion" del recurso de versión (tipo 16).
pub fn file_version(b: &[u8], res: &[Resource]) -> String {
    let Some(v) = res.iter().find(|r| r.path.first() == Some(&ResKey::Id(16))) else { return String::new() };
    let units: Vec<u16> = b[v.off..v.off + v.size].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let s = String::from_utf16_lossy(&units);
    let Some(i) = s.find("FileVersion") else { return String::new() };
    s[i + 11..].trim_start_matches('\0').chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect()
}

/// Recalcula el checksum de la cabecera (el mismo que escriben las herramientas de Windows).
pub fn fix_checksum(b: &mut [u8]) {
    let e = u32le(b, 0x3c) as usize;
    let at = e + 24 + 64;
    let mut sum: u64 = 0;
    let n = b.len() & !1;
    let mut i = 0;
    while i < n {
        if i != at && i != at + 2 {
            sum += u16le(b, i) as u64;
            sum = (sum & 0xffff) + (sum >> 16);
        }
        i += 2;
    }
    if b.len() & 1 == 1 {
        sum += b[b.len() - 1] as u64;
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum = (sum & 0xffff) + (sum >> 16);
    let v = (sum as u32).wrapping_add(b.len() as u32);
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
