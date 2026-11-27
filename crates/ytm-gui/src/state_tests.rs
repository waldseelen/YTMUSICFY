use super::*;

#[test]
fn gui_app_config_defaults_missing_governance_fields() {
    let cfg: GuiAppConfig = toml::from_str("").expect("deserialize empty config");
    assert_eq!(cfg.mutation_policy, MutationPolicy::StrictLock);
    assert!(!cfg.auto_refresh_browser_cookies);
    assert_eq!(cfg.theme_mode, ThemeMode::System);
    assert_eq!(cfg.accent_palette, AccentPalette::Emerald);
    assert!(cfg.dynamic_accent);
    assert_eq!(cfg.ui_lang, UiLang::System);
    assert!(cfg.greetings_enabled);
    assert!((cfg.ticker_speed - 48.0).abs() < f32::EPSILON);
    assert!(!cfg.reduced_motion);
    assert!(cfg.preferred_external_browser.is_none());
    assert!(cfg.recent_searches.is_empty());
    assert!(cfg.tray_enabled);
    assert!(cfg.close_to_tray);
    assert!(!cfg.mini_player_opacity_on_idle);
    assert!(cfg.mini_player_layouts.is_empty());
}

#[test]
fn v0_1_1_config_toml_fixture_loads_with_dalga_defaults() {
    let legacy_toml = include_str!("../../../config/config.example.toml");
    let cfg: GuiAppConfig = toml::from_str(legacy_toml).expect("deserialize v0.1.1 fixture");
    assert_eq!(cfg.innertube.hl, "en");
    assert_eq!(cfg.innertube.gl, "US");
    assert_eq!(cfg.innertube.mutation_chunk_size, 50);
    assert_eq!(cfg.theme_mode, ThemeMode::System);
    assert_eq!(cfg.accent_palette, AccentPalette::Emerald);
    assert!(cfg.dynamic_accent);
    assert_eq!(cfg.ui_lang, UiLang::System);
    assert!(cfg.greetings_enabled);
    assert!((cfg.ticker_speed - 48.0).abs() < f32::EPSILON);
    assert!(!cfg.reduced_motion);
}

#[test]
fn draft_revision_bump_and_session_state_default() {
    let mut rev = DraftRevision::default();
    assert_eq!(rev, DraftRevision(0));
    assert_eq!(rev.bump(), DraftRevision(1));
    assert_eq!(rev.bump(), DraftRevision(2));
    assert_eq!(SessionState::default(), SessionState::Unauthenticated);
}
