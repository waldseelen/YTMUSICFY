use crate::contracts::UiLang;

#[path = "i18n_en.rs"]
mod en;
#[path = "i18n_tr.rs"]
mod tr;

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

/// Canonical catalog keys for localized UI copy (Turkish & English).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Back,
    Forward,
    SearchPlaceholder,
    SearchClear,
    SearchEnterHint,
    SearchTopMatch,
    SearchSongs,
    SearchArtist,
    SearchAlbum,
    SearchRecent,
    SearchSuggestions,
    ThemeModeLabel,
    ThemeSystem,
    ThemeDark,
    ThemeLight,
    AccentLabel,
    AccentEmerald,
    AccentGold,
    AccentYtmPink,
    DynamicAccentLabel,
    MiniPlayerSoon,
    SettingsTitle,
    SettingsLangLabel,
    LangSystem,
    LangEnglish,
    LangTurkish,
    SettingsSave,
    SettingsSavedToast,
    SettingsSafetyHeading,
    SettingsStrictLock,
    SettingsQuickLike,
    SettingsAutoRefreshCookies,
    SettingsProtocolHeading,
    SettingsEnrichHeading,
    SettingsFastMode,
    SettingsDeezerFallback,
    SettingsListenBrainzFallback,
    SettingsLastfmKey,
    SettingsRibbonHeading,
    SettingsGreetingsToggle,
    SettingsTickerSpeed,
    SettingsReducedMotion,
    SettingsBrowserLabel,
    SettingsBrowserDefault,
    AuthTitle,
    AuthNotSignedIn,
    AuthRefreshSession,
    AuthRefreshingWait,
    AuthImportBrowser,
    AuthImportClipboard,
    AuthVerifySave,
    AuthClear,
    AuthImportFile,
    AuthAudioQuality,
    AuthBannerExpired,
    SidebarLibrary,
    SidebarLists,
    SidebarAllTracks,
    SidebarFilterHint,
    SidebarTrackFilterHint,
    SidebarNewPlaylistHint,
    SidebarStageCreate,
    SidebarStudioTools,
    SidebarHomeExplore,
    SidebarCatalogSearch,
    SidebarInspector,
    SidebarDedupe,
    SidebarSort,
    SidebarSetOps,
    SidebarHealth,
    SidebarGenreSplit,
    SidebarSnapshots,
    SidebarSync,
    SidebarAuth,
    SidebarSettings,
    PlayerNotPlaying,
    PlayerPlayHint,
    PlayerPlay,
    PlayerPause,
    PlayerPrev,
    PlayerNext,
    PlayerStop,
    PlayerShuffle,
    PlayerRepeat,
    PlayerMute,
    PlayerQueue,
    PlayerLyrics,
    PlayerRadio,
    PlayerLike,
    PlayerUnlike,
    PlayerOpenCover,
    DrawerClose,
    RibbonHistoryTitle,
    RibbonEmpty,
    RibbonDismiss,
    RibbonUndo,
    RibbonOpenSync,
    OpenInYtm,
    OpenInYtmAtTime,
    SearchOnYoutube,
    CopyLink,
    LinkCopiedToast,
    OfflineUsingCache,
    SyncDrawer,
}

impl Key {
    pub const ALL: &'static [Self] = &[
        Self::Back,
        Self::Forward,
        Self::SearchPlaceholder,
        Self::SearchClear,
        Self::SearchEnterHint,
        Self::SearchTopMatch,
        Self::SearchSongs,
        Self::SearchArtist,
        Self::SearchAlbum,
        Self::SearchRecent,
        Self::SearchSuggestions,
        Self::ThemeModeLabel,
        Self::ThemeSystem,
        Self::ThemeDark,
        Self::ThemeLight,
        Self::AccentLabel,
        Self::AccentEmerald,
        Self::AccentGold,
        Self::AccentYtmPink,
        Self::DynamicAccentLabel,
        Self::MiniPlayerSoon,
        Self::SettingsTitle,
        Self::SettingsLangLabel,
        Self::LangSystem,
        Self::LangEnglish,
        Self::LangTurkish,
        Self::SettingsSave,
        Self::SettingsSavedToast,
        Self::SettingsSafetyHeading,
        Self::SettingsStrictLock,
        Self::SettingsQuickLike,
        Self::SettingsAutoRefreshCookies,
        Self::SettingsProtocolHeading,
        Self::SettingsEnrichHeading,
        Self::SettingsFastMode,
        Self::SettingsDeezerFallback,
        Self::SettingsListenBrainzFallback,
        Self::SettingsLastfmKey,
        Self::SettingsRibbonHeading,
        Self::SettingsGreetingsToggle,
        Self::SettingsTickerSpeed,
        Self::SettingsReducedMotion,
        Self::SettingsBrowserLabel,
        Self::SettingsBrowserDefault,
        Self::AuthTitle,
        Self::AuthNotSignedIn,
        Self::AuthRefreshSession,
        Self::AuthRefreshingWait,
        Self::AuthImportBrowser,
        Self::AuthImportClipboard,
        Self::AuthVerifySave,
        Self::AuthClear,
        Self::AuthImportFile,
        Self::AuthAudioQuality,
        Self::AuthBannerExpired,
        Self::SidebarLibrary,
        Self::SidebarLists,
        Self::SidebarAllTracks,
        Self::SidebarFilterHint,
        Self::SidebarTrackFilterHint,
        Self::SidebarNewPlaylistHint,
        Self::SidebarStageCreate,
        Self::SidebarStudioTools,
        Self::SidebarHomeExplore,
        Self::SidebarCatalogSearch,
        Self::SidebarInspector,
        Self::SidebarDedupe,
        Self::SidebarSort,
        Self::SidebarSetOps,
        Self::SidebarHealth,
        Self::SidebarGenreSplit,
        Self::SidebarSnapshots,
        Self::SidebarSync,
        Self::SidebarAuth,
        Self::SidebarSettings,
        Self::PlayerNotPlaying,
        Self::PlayerPlayHint,
        Self::PlayerPlay,
        Self::PlayerPause,
        Self::PlayerPrev,
        Self::PlayerNext,
        Self::PlayerStop,
        Self::PlayerShuffle,
        Self::PlayerRepeat,
        Self::PlayerMute,
        Self::PlayerQueue,
        Self::PlayerLyrics,
        Self::PlayerRadio,
        Self::PlayerLike,
        Self::PlayerUnlike,
        Self::PlayerOpenCover,
        Self::DrawerClose,
        Self::RibbonHistoryTitle,
        Self::RibbonEmpty,
        Self::RibbonDismiss,
        Self::RibbonUndo,
        Self::RibbonOpenSync,
        Self::OpenInYtm,
        Self::OpenInYtmAtTime,
        Self::SearchOnYoutube,
        Self::CopyLink,
        Self::LinkCopiedToast,
        Self::OfflineUsingCache,
        Self::SyncDrawer,
    ];
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
