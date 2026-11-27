use crate::contracts::UiLang;

#[path = "i18n_en.rs"]
mod en;
#[path = "i18n_keys.rs"]
mod keys;
#[path = "i18n_tr.rs"]
mod tr;

pub use keys::Key;

/// Resolved concrete UI language (never `System`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolvedLang {
    #[default]
    En,
    Tr,
}

/// Resolve `UiLang` against an optional POSIX locale string (e.g. `LANG="tr_TR.UTF-8"`).
#[must_use]
pub fn resolve_lang_with_locale(pref: UiLang, locale: Option<&str>) -> ResolvedLang {
    match pref {
        UiLang::En => ResolvedLang::En,
        UiLang::Tr => ResolvedLang::Tr,
        UiLang::System => {
            let is_tr = locale.is_some_and(|raw| {
                let norm = raw.trim().to_ascii_lowercase();
                norm == "tr" || norm.starts_with("tr_") || norm.starts_with("tr-")
            });
            if is_tr {
                ResolvedLang::Tr
            } else {
                ResolvedLang::En
            }
        }
    }
}

/// Resolve `UiLang` using host process environment variables (`LC_ALL` / `LC_MESSAGES` / `LANG`).
#[must_use]
pub fn resolve_lang(pref: UiLang) -> ResolvedLang {
    let locale = std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("LC_MESSAGES").ok().filter(|v| !v.is_empty()))
        .or_else(|| std::env::var("LANG").ok().filter(|v| !v.is_empty()));
    resolve_lang_with_locale(pref, locale.as_deref())
}

/// Look up localized text for `key` in `lang`.
#[must_use]
pub const fn tr(lang: ResolvedLang, key: Key) -> &'static str {
    match lang {
        ResolvedLang::En => en::lookup(key),
        ResolvedLang::Tr => tr::lookup(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_i18n_key_has_non_empty_en_and_tr_entries() {
        for &key in Key::ALL {
            assert!(!tr(ResolvedLang::En, key).trim().is_empty(), "{key:?} EN");
            assert!(!tr(ResolvedLang::Tr, key).trim().is_empty(), "{key:?} TR");
        }
    }

    #[test]
    fn system_lang_resolves_turkish_locale_and_falls_back_to_english() {
        assert_eq!(
            resolve_lang_with_locale(UiLang::System, Some("tr_TR.UTF-8")),
            ResolvedLang::Tr
        );
        assert_eq!(
            resolve_lang_with_locale(UiLang::System, Some("en_US.UTF-8")),
            ResolvedLang::En
        );
        assert_eq!(
            resolve_lang_with_locale(UiLang::System, None),
            ResolvedLang::En
        );
        assert_eq!(
            resolve_lang_with_locale(UiLang::Tr, Some("en_US.UTF-8")),
            ResolvedLang::Tr
        );
    }
}
