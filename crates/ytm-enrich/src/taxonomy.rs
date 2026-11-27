use std::fs;
use std::path::Path;

use crate::error::EnrichError;
use crate::models::GenreTaxonomyConfig;

#[derive(Debug, Clone)]
struct CompiledRule {
    pattern_lower: String,
    parent: String,
    order: usize,
}

/// Hierarchical longest-pattern-first genre classifier backed by `genres.toml`.
#[derive(Debug, Clone)]
pub struct GenreMatcher {
    default_genre: String,
    min_bucket_tracks: usize,
    rules: Vec<CompiledRule>,
}

impl GenreMatcher {
    /// Builds a compiled matcher sorting patterns longest-first across all parent buckets.
    #[must_use]
    pub fn from_config(config: GenreTaxonomyConfig) -> Self {
        let mut compiled = Vec::new();
        let mut order = 0usize;

        for rule in config.rules {
            for pat in rule.patterns {
                let pat_lower = pat.trim().to_lowercase();
                if !pat_lower.is_empty() {
                    compiled.push(CompiledRule {
                        pattern_lower: pat_lower,
                        parent: rule.parent.clone(),
                        order,
                    });
                    order += 1;
                }
            }
        }

        compiled.sort_by(|a, b| {
            b.pattern_lower
                .len()
                .cmp(&a.pattern_lower.len())
                .then_with(|| a.order.cmp(&b.order))
        });

        Self {
            default_genre: config.default_genre,
            min_bucket_tracks: config.min_bucket_tracks,
            rules: compiled,
        }
    }

    /// Parses taxonomy TOML from an in-memory string.
    pub fn from_toml_str(raw: &str) -> Result<Self, EnrichError> {
        let cfg: GenreTaxonomyConfig = toml::from_str(raw)?;
        Ok(Self::from_config(cfg))
    }

    /// Reads and parses a `genres.toml` file from disk.
    pub fn from_toml_path(path: &Path) -> Result<Self, EnrichError> {
        let raw = fs::read_to_string(path).map_err(|source| EnrichError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&raw)
    }

    /// Built-in default taxonomy matcher used when no custom `genres.toml` is configured.
    #[must_use]
    pub fn built_in_default() -> Self {
        Self::from_toml_str(DEFAULT_GENRES_TOML).expect("built-in default taxonomy is valid TOML")
    }

    #[must_use]
    pub fn default_genre(&self) -> &str {
        &self.default_genre
    }

    #[must_use]
    pub fn min_bucket_tracks(&self) -> usize {
        self.min_bucket_tracks
    }

    /// Maps raw provider genre/folksonomy tags to a canonical parent bucket using longest match.
    #[must_use]
    pub fn classify(&self, raw_tags: &[String]) -> String {
        let normalized_tags: Vec<String> = raw_tags
            .iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();

        if normalized_tags.is_empty() {
            return self.default_genre.clone();
        }

        // Exact matches take priority in longest-pattern order.
        for rule in &self.rules {
            if normalized_tags.iter().any(|tag| tag == &rule.pattern_lower) {
                return rule.parent.clone();
            }
        }

        // Boundary-safe or multi-char substring fallback in longest-pattern order.
        for rule in &self.rules {
            if normalized_tags
                .iter()
                .any(|tag| tag_matches_pattern(tag, &rule.pattern_lower))
            {
                return rule.parent.clone();
            }
        }

        self.default_genre.clone()
    }
}

fn tag_matches_pattern(tag: &str, pattern: &str) -> bool {
    if tag == pattern {
        return true;
    }
    if pattern.len() >= 4 && tag.contains(pattern) {
        return true;
    }
    tag.split(|c: char| !c.is_alphanumeric())
        .any(|token| token == pattern)
}

const DEFAULT_GENRES_TOML: &str = r#"
default_genre = "Other"
min_bucket_tracks = 5

[[rules]]
parent = "Rock"
patterns = ["rock", "alt-rock", "alternative rock", "indie rock", "post-rock", "post-punk", "prog rock", "progressive rock", "classic rock", "hard rock", "punk", "punk rock", "anatolian rock", "turkish rock", "grunge", "shoegaze", "psychedelic rock"]

[[rules]]
parent = "Metal"
patterns = ["metal", "heavy metal", "death metal", "melodic death metal", "black metal", "thrash metal", "progressive metal", "metalcore", "doom metal", "nu metal", "industrial metal"]

[[rules]]
parent = "Electronic"
patterns = ["electronic", "electronica", "edm", "house", "deep house", "tech house", "french house", "techno", "trance", "synthwave", "darksynth", "cyberpunk", "retrowave", "vaporwave", "chillwave", "future funk", "industrial", "drum and bass", "dnb", "ambient", "downtempo", "trip-hop", "idm", "dubstep", "electro", "lo-fi", "lofi", "phonk"]

[[rules]]
parent = "Hip-Hop"
patterns = ["hip-hop", "hip hop", "rap", "trap", "boom bap", "drill", "turkish hip hop", "turkish rap"]

[[rules]]
parent = "Pop"
patterns = ["pop", "synthpop", "indie pop", "dream pop", "turkish pop", "k-pop", "j-pop"]

[[rules]]
parent = "R&B"
patterns = ["r&b", "rnb", "contemporary r&b", "neo soul", "soul", "funk"]

[[rules]]
parent = "Jazz"
patterns = ["jazz", "jazz fusion", "bebop", "fusion", "nu jazz", "smooth jazz", "blues"]

[[rules]]
parent = "Soundtrack"
patterns = ["soundtrack", "video game music", "vgm", "game ost", "film score", "score", "anime"]

[[rules]]
parent = "Folk"
patterns = ["folk", "indie folk", "acoustic", "country", "anatolian folk", "türkü", "turkish folk"]

[[rules]]
parent = "Classical"
patterns = ["classical", "contemporary classical", "neoclassical", "baroque", "orchestral", "symphony", "chamber music"]
"#;
