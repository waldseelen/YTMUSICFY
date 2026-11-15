use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;
use ytm_core::{MusicVideoType, Track};

/// Normalized representation of a track used for duplicate detection, set matching, and health upgrades.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedTrack {
    pub artist_norm: String,
    pub title_norm: String,
    pub is_clean_title: bool,
}

impl NormalizedTrack {
    /// Combined canonical key for exact-normalized comparisons.
    pub fn canonical_key(&self) -> String {
        format!("{}::{}", self.artist_norm, self.title_norm)
    }
}

struct NormalizeRegexes {
    bracket_noise: Regex,
    feat_clause: Regex,
    trailing_noise: Regex,
    artist_topic: Regex,
    ampersand: Regex,
}

fn regexes() -> &'static NormalizeRegexes {
    static RE: OnceLock<NormalizeRegexes> = OnceLock::new();
    RE.get_or_init(|| NormalizeRegexes {
        bracket_noise: Regex::new(
            r"(?i)[\(\[]([^\)\]]*\b(official|video|audio|lyric|lyrics|hd|4k|hq|visualizer|visualiser|mv|m/v|music\s+video|live|acoustic|remaster(?:ed)?|sped\s+up|slowed|reverb|radio\s+edit|explicit|clean|version|topic)\b[^\)\]]*)[\)\]]",
        )
        .expect("valid bracket_noise regex"),
        feat_clause: Regex::new(
            r"(?i)(?:[\(\[]\s*(?:feat\.?|ft\.?|featuring)\s+[^\)\]]+[\)\]]|\s+(?:feat\.?|ft\.?|featuring)\s+.+$)",
        )
        .expect("valid feat_clause regex"),
        trailing_noise: Regex::new(
            r"(?i)\s*[-–—|]\s*(?:official(?:\s+music)?\s+(?:video|audio)|lyrics?(?:\s+video)?|visualizer|visualiser|audio|live|acoustic|remaster(?:ed)?(?:\s+\d{4})?|sped\s+up|slowed(?:\s*\+\s*reverb)?|radio\s+edit)\s*$",
        )
        .expect("valid trailing_noise regex"),
        artist_topic: Regex::new(r"(?i)\s*-\s*topic\s*$").expect("valid artist_topic regex"),
        ampersand: Regex::new(r"\s*[&+]\s*").expect("valid ampersand regex"),
    })
}

fn is_variant_suffix(right: &str) -> bool {
    let re = regexes();
    let without_brackets = re.bracket_noise.replace_all(right, " ");
    let core = without_brackets.trim().to_ascii_lowercase();
    const EXACT_SUFFIXES: &[&str] = &[
        "remix",
        "edit",
        "live",
        "version",
        "acoustic",
        "instrumental",
        "sped up",
        "slowed",
        "reverb",
        "slowed + reverb",
        "radio edit",
        "remaster",
        "remastered",
    ];
    EXACT_SUFFIXES.iter().any(|s| core == *s)
        || core.ends_with(" remix")
        || core.ends_with(" edit")
        || core.ends_with(" version")
}

fn split_dash_pair(raw: &str) -> Option<(&str, &str)> {
    for sep in [" - ", " – ", " — "] {
        if let Some((left, right)) = raw.split_once(sep) {
            let l = left.trim();
            let r = right.trim();
            if !l.is_empty() && !r.is_empty() {
                return Some((l, r));
            }
        }
    }
    None
}

fn normalize_text_core(input: &str) -> String {
    let nfkc: String = input.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let with_and = regexes().ampersand.replace_all(&lower, " and ");
    let cleaned: String = with_and
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else if c == '\'' || c == '’' {
                // Drop apostrophes without introducing spaces ("don't" -> "dont").
                '\0'
            } else {
                ' '
            }
        })
        .filter(|&c| c != '\0')
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_title_noise(raw_title: &str) -> (String, bool) {
    let re = regexes();
    let mut current = raw_title.nfkc().collect::<String>();
    let mut had_noise = false;

    loop {
        let next = re.bracket_noise.replace_all(&current, " ").into_owned();
        if next.trim() == current.trim() {
            break;
        }
        had_noise = true;
        current = next;
    }

    let after_trailing = re.trailing_noise.replace_all(&current, "").into_owned();
    if after_trailing.trim() != current.trim() {
        had_noise = true;
        current = after_trailing;
    }

    let after_feat = re.feat_clause.replace_all(&current, " ").into_owned();
    let cleaned = normalize_text_core(&after_feat);
    (cleaned, !had_noise)
}

fn normalize_artist_raw(raw_artist: &str) -> String {
    let nfkc: String = raw_artist.nfkc().collect();
    let without_topic = regexes().artist_topic.replace(&nfkc, "");
    let without_feat = regexes().feat_clause.replace(&without_topic, "");
    normalize_text_core(&without_feat)
}

/// Normalizes a `Track` into canonical lowercase NFKC strings for matching.
pub fn normalize_track(track: &Track) -> NormalizedTrack {
    let primary_artist = track.primary_artist_name().unwrap_or_default().trim();
    let primary_norm = normalize_artist_raw(primary_artist);
    let raw_title = track.title.trim();

    let (chosen_artist, chosen_title) = if let Some((left, right)) = split_dash_pair(raw_title) {
        let left_norm = normalize_artist_raw(left);
        let is_ugc = track.music_video_type == MusicVideoType::UserGenerated;
        if primary_norm.is_empty()
            || left_norm == primary_norm
            || (is_ugc && !is_variant_suffix(right))
        {
            (left_norm, right)
        } else {
            (primary_norm, raw_title)
        }
    } else {
        (primary_norm, raw_title)
    };

    let (title_norm, is_clean_title) = strip_title_noise(chosen_title);
    NormalizedTrack {
        artist_norm: chosen_artist,
        title_norm,
        is_clean_title,
    }
}
