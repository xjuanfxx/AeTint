//! Idioma de la interfaz: inglés por defecto, español opcional.

use std::sync::atomic::{AtomicBool, Ordering};

static ES: AtomicBool = AtomicBool::new(false);

pub fn set_lang(lang: &str) {
    ES.store(lang == "es", Ordering::Relaxed);
}

pub fn es() -> bool {
    ES.load(Ordering::Relaxed)
}

/// Texto fijo en inglés / español.
pub fn t(en: &'static str, es_: &'static str) -> &'static str {
    if es() {
        es_
    } else {
        en
    }
}

/// `tr!("English {}", "Español {}", x)` → String en el idioma actual.
#[macro_export]
macro_rules! tr {
    ($en:literal, $es:literal $(, $a:expr)* $(,)?) => {
        if $crate::i18n::es() { format!($es $(, $a)*) } else { format!($en $(, $a)*) }
    };
}
