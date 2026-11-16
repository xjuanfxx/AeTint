//! Reescribir un JSON incrustado en una DLL sin cambiar su tamaño en bytes.
//!
//! Los recursos de Adobe vienen con mucha sangría. Se lee el JSON, se cambian los valores, se vuelve a escribir
//! compacto (sin sangría, mismas claves en el mismo orden) y el espacio que sobra se rellena con espacios antes de
//! la última llave: sigue siendo JSON válido y el recurso ocupa exactamente lo mismo.

use serde_json::Value;

pub fn rewrite_same_len(orig: &[u8], f: impl FnOnce(&mut Value) -> Result<(), String>) -> Result<Vec<u8>, String> {
    let start = orig.iter().position(|&c| c == b'{').ok_or_else(|| tr!("the resource isn't JSON", "el recurso no es JSON"))?;
    let end = orig.iter().rposition(|&c| c == b'}').ok_or_else(|| tr!("the resource isn't JSON", "el recurso no es JSON"))?;
    let body = std::str::from_utf8(&orig[start..=end]).map_err(|_| tr!("the JSON isn't UTF-8", "el JSON no es UTF-8"))?;
    let mut v: Value = serde_json::from_str(body).map_err(|e| tr!("unreadable JSON: {}", "JSON ilegible: {}", e))?;
    f(&mut v)?;
    let compact = serde_json::to_string(&v).map_err(|e| e.to_string())?;
    let room = end + 1 - start;
    if compact.len() > room {
        return Err(tr!("the new JSON doesn't fit ({} > {} bytes)", "el JSON nuevo no cabe ({} > {} bytes)", compact.len(), room));
    }
    let mut out = Vec::with_capacity(orig.len());
    out.extend_from_slice(&orig[..start]);
    out.extend_from_slice(&compact.as_bytes()[..compact.len() - 1]);
    out.resize(out.len() + room - compact.len(), b' ');
    out.push(b'}');
    out.extend_from_slice(&orig[end + 1..]);
    debug_assert_eq!(out.len(), orig.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_size_and_order() {
        let src = b"\xEF\xBB\xBF{\n\t\"b\": \"rgb(0, 0, 0)\",\n\t\"a\": 1\n}\n\0";
        let out = rewrite_same_len(src, |v| {
            v["b"] = Value::String("rgb(255, 255, 255)".into());
            Ok(())
        })
        .unwrap();
        assert_eq!(out.len(), src.len());
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains(r#"{"b":"rgb(255, 255, 255)","a":1"#));
        assert!(out.ends_with(b"}\n\0"));
    }
}
