/// Detect the user's preferred language code ("en", "es", etc.)
/// and set the rust-i18n locale accordingly.
///
/// Detection order:
///   1. LANG / LC_ALL / LC_MESSAGES env vars (set by macOS and all Unix shells)
///   2. Fallback: "en"
pub fn init() {
    let locale = detect_locale();
    rust_i18n::set_locale(&locale);
    log::debug!("i18n locale: {locale}");
}

fn detect_locale() -> String {
    for var in ["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(var) {
            // LANG is typically "en_US.UTF-8" — extract the 2-letter language code.
            let code = val
                .split(['_', '.'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if !code.is_empty()
                && code != "c"
                && code != "posix"
                && rust_i18n::available_locales!()
                    .iter()
                    .any(|l| l.as_ref() == code.as_str())
            {
                return code;
            }
        }
    }
    "en".to_string()
}
