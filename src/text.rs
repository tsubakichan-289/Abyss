use std::collections::HashMap;
use std::env;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::Deserialize;

#[derive(Deserialize)]
struct TextBundle {
    entries: HashMap<String, String>,
}

static EN: OnceLock<TextBundle> = OnceLock::new();
static JA: OnceLock<TextBundle> = OnceLock::new();
static LA: OnceLock<TextBundle> = OnceLock::new();
static LANG: AtomicU8 = AtomicU8::new(0); // 0=en, 1=ja, 2=la

fn en_bundle() -> &'static TextBundle {
    EN.get_or_init(|| {
        serde_json::from_str(include_str!("../data/text_en.json"))
            .expect("failed to parse data/text_en.json")
    })
}

fn ja_bundle() -> &'static TextBundle {
    JA.get_or_init(|| {
        serde_json::from_str(include_str!("../data/text_ja.json"))
            .expect("failed to parse data/text_ja.json")
    })
}

fn la_bundle() -> &'static TextBundle {
    LA.get_or_init(|| {
        serde_json::from_str(include_str!("../data/text_la.json"))
            .expect("failed to parse data/text_la.json")
    })
}

fn lang_index(code: &str) -> Option<u8> {
    match code {
        "en" => Some(0),
        "ja" => Some(1),
        "la" => Some(2),
        _ => None,
    }
}

fn active() -> &'static TextBundle {
    match LANG.load(Ordering::Relaxed) {
        1 => ja_bundle(),
        2 => la_bundle(),
        _ => en_bundle(),
    }
}

pub(crate) fn init_from_env() {
    let code = env::var("ABYSS_LANG").unwrap_or_else(|_| "en".to_string());
    let _ = set_lang(&code);
}

pub(crate) fn available_languages() -> &'static [(&'static str, &'static str)] {
    &[("en", "English"), ("ja", "Japanese"), ("la", "Latin")]
}

pub(crate) fn set_lang(code: &str) -> bool {
    let normalized = code.to_ascii_lowercase();
    if let Some(idx) = lang_index(&normalized) {
        LANG.store(idx, Ordering::Relaxed);
        true
    } else {
        false
    }
}

pub(crate) fn current_lang() -> &'static str {
    match LANG.load(Ordering::Relaxed) {
        1 => "ja",
        2 => "la",
        _ => "en",
    }
}

pub(crate) fn tr(key: &str) -> &'static str {
    if let Some(v) = active().entries.get(key).map(String::as_str) {
        v
    } else {
        Box::leak(key.to_string().into_boxed_str())
    }
}

pub(crate) fn trf(key: &str, args: &[(&str, String)]) -> String {
    let mut out = tr(key).to_string();
    for (name, value) in args {
        let pat = format!("{{{name}}}");
        out = out.replace(&pat, value);
    }
    out
}

pub(crate) fn trf_map(key: &str, args: &[(String, String)]) -> String {
    let mut out = tr(key).to_string();
    for (name, value) in args {
        let pat = format!("{{{name}}}");
        out = out.replace(&pat, value);
    }
    out
}
