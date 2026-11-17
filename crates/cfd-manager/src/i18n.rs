use cfd_core::config::UiLang;
use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: UiLang) {
    let v = match lang {
        UiLang::ZhTw => 0,
        UiLang::En => 1,
    };
    LANG.store(v, Ordering::Relaxed);
}

pub fn get_lang() -> UiLang {
    if LANG.load(Ordering::Relaxed) == 1 {
        UiLang::En
    } else {
        UiLang::ZhTw
    }
}

#[inline]
pub fn tr<'a>(zh: &'a str, en: &'a str) -> &'a str {
    if LANG.load(Ordering::Relaxed) == 1 {
        en
    } else {
        zh
    }
}
