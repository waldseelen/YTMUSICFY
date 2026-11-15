use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ytm_core::ClientConfig;
use ytm_enrich::{EnrichmentConfig, GenreMatcher};
use ytm_store::AppPaths;

/// Top-level application configuration mapped from `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub innertube: ClientConfig,
    pub enrichment: EnrichmentConfig,
}

/// Loads `config.toml` if present, applying `YTM_LASTFM_API_KEY` env override.
pub fn load_app_config(paths: &AppPaths) -> Result<AppConfig> {
    let config_path = paths.config_file();
    let mut cfg = if config_path.exists() {
        let raw = fs::read_to_string(&config_path)
            .with_context(|| format!("reading config file {}", config_path.display()))?;
        toml::from_str::<AppConfig>(&raw)
            .with_context(|| format!("parsing TOML in {}", config_path.display()))?
    } else {
        AppConfig::default()
    };

    if cfg.enrichment.lastfm_api_key.is_none()
        && let Ok(env_key) = std::env::var("YTM_LASTFM_API_KEY")
    {
        let trimmed = env_key.trim();
        if !trimmed.is_empty() {
            cfg.enrichment.lastfm_api_key = Some(trimmed.to_string());
        }
    }

    Ok(cfg)
}

/// Loads custom `--config` taxonomy TOML, user `genres.toml`, or built-in defaults.
pub fn load_genre_matcher(custom_path: Option<&Path>, paths: &AppPaths) -> Result<GenreMatcher> {
    if let Some(path) = custom_path {
        return GenreMatcher::from_toml_path(path)
            .with_context(|| format!("loading genre taxonomy {}", path.display()));
    }
    let user_genres = paths.genres_file();
    if user_genres.exists() {
        return GenreMatcher::from_toml_path(&user_genres)
            .with_context(|| format!("loading genre taxonomy {}", user_genres.display()));
    }
    Ok(GenreMatcher::built_in_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_toml_deserializes_into_app_config() {
        let raw = include_str!("../../../config/config.example.toml");
        let parsed: AppConfig = toml::from_str(raw).expect("valid config.example.toml");
        assert_eq!(parsed.innertube.hl, "en");
        assert_eq!(parsed.innertube.mutation_chunk_size, 50);
        assert_eq!(parsed.enrichment.musicbrainz_interval_ms, 1100);
    }
}
