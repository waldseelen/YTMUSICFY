use ytm_core::{MusicVideoType, Track};

/// Normalized key components and display strings for cache lookup and provider queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedTrackParts {
    pub cache_key: String,
    pub artist_norm: String,
    pub title_norm: String,
    pub artist_display: String,
    pub title_display: String,
}

fn compose_latin_mark(base: char, mark: char) -> Option<char> {
    const LATIN: &str = "a\u{0300}àa\u{0301}áa\u{0302}âa\u{0303}ãa\u{0308}äa\u{030A}åc\u{0327}çe\u{0300}èe\u{0301}ée\u{0302}êe\u{0308}ëi\u{0300}ìi\u{0301}íi\u{0302}îi\u{0308}ïn\u{0303}ño\u{0300}òo\u{0301}óo\u{0302}ôo\u{0303}õo\u{0308}öu\u{0300}ùu\u{0301}úu\u{0302}ûu\u{0308}üy\u{0301}ýy\u{0308}ÿ";
    let mut it = LATIN.chars();
    while let (Some(b), Some(m), Some(c)) = (it.next(), it.next(), it.next()) {
        if b == base.to_ascii_lowercase() && m == mark {
            return if base.is_ascii_uppercase() {
                c.to_uppercase().next()
            } else {
                Some(c)
            };
        }
    }
    None
}

fn nfkc_fold(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for raw in input.chars() {
        let expansion = match raw {
            'ﬁ' => Some("fi"),
            'ﬂ' => Some("fl"),
            'ﬀ' => Some("ff"),
            'ﬃ' => Some("ffi"),
            'ﬄ' => Some("ffl"),
            '™' => Some("tm"),
            '…' => Some("..."),
            _ => None,
        };
        if let Some(rep) = expansion {
            out.push_str(rep);
            continue;
        }
        let ch = match raw {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32((raw as u32) - 0xFEE0).unwrap_or(raw),
            '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200B}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}' => ' ',
            '¹' | '₁' => '1',
            '²' | '₂' => '2',
            '³' | '₃' => '3',
            '⁰' | '₀' => '0',
            other => other,
        };
        if ('\u{0300}'..='\u{036F}').contains(&ch)
            && let Some(prev) = out.chars().last()
            && let Some(composed) = compose_latin_mark(prev, ch)
        {
            out.pop();
            out.push(composed);
            continue;
        }
        out.push(ch);
    }
    out
}

fn is_noise_bracket(inner: &str) -> bool {
    const WORDS: &str = "official video audio lyric lyrics hd 4k hq visualizer visualiser mv live acoustic remaster remastered slowed reverb explicit clean version topic";
    let lower = inner.trim().to_lowercase();
    ["feat ", "feat.", "ft ", "ft.", "featuring "]
        .iter()
        .any(|p| lower.starts_with(p))
        || ["music video", "sped up", "radio edit", "m/v"]
            .iter()
            .any(|p| lower.contains(p))
        || lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|tok| !tok.is_empty() && WORDS.split(' ').any(|w| w == tok))
}

fn strip_bracket_noise(input: &str) -> String {
    let mut cur = input.to_string();
    let mut pos = 0;
    while let Some((rel, open)) = cur[pos..]
        .char_indices()
        .find(|&(_, c)| c == '(' || c == '[')
    {
        let start = pos + rel;
        let close = if open == '(' { ')' } else { ']' };
        let body_start = start + 1;
        let Some(rel_end) = cur[body_start..].find(close) else {
            pos = body_start;
            continue;
        };
        let end = body_start + rel_end;
        if is_noise_bracket(&cur[body_start..end]) {
            cur.replace_range(start..=end, " ");
            pos = start;
        } else {
            pos = end + 1;
        }
    }
    cur
}

fn strip_feat_clause(input: &str) -> &str {
    let mut prev_ws = false;
    for (idx, ch) in input.char_indices() {
        let tail = &input[idx..];
        let matches_feat = ["feat.", "feat ", "ft.", "ft ", "featuring "]
            .iter()
            .any(|p| {
                tail.get(..p.len())
                    .is_some_and(|s| s.eq_ignore_ascii_case(p))
            })
            || tail.eq_ignore_ascii_case("feat")
            || tail.eq_ignore_ascii_case("ft");
        if prev_ws && matches_feat {
            return input[..idx].trim_end();
        }
        prev_ws = ch.is_whitespace();
    }
    input
}

fn collapse_ws_lower(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn strip_trailing_noise(input: &str) -> &str {
    const TRAILS: &str = "official video|official music video|official audio|official music audio|lyric|lyrics|lyric video|lyrics video|visualizer|visualiser|audio|live|acoustic|remaster|remastered|sped up|slowed|slowed + reverb|slowed+reverb|radio edit";
    let Some((left, right)) = input.rsplit_once(['-', '–', '—', '|']) else {
        return input;
    };
    let norm = collapse_ws_lower(right);
    let remaster_year = norm
        .strip_prefix("remaster ")
        .or_else(|| norm.strip_prefix("remastered "))
        .is_some_and(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()));
    if (TRAILS.split('|').any(|t| t == norm) || remaster_year) && !left.trim().is_empty() {
        left.trim_end()
    } else {
        input
    }
}

fn is_variant_suffix(right: &str) -> bool {
    const VARIANTS: &str = "remix|edit|live|version|acoustic|instrumental|sped up|slowed|reverb|slowed + reverb|radio edit|remaster|remastered";
    let core = collapse_ws_lower(&strip_bracket_noise(right));
    VARIANTS.split('|').any(|v| v == core)
        || [" remix", " edit", " version"]
            .iter()
            .any(|s| core.ends_with(s))
}

fn normalize_text_core(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in nfkc_fold(input).to_lowercase().chars() {
        if ch == '&' || ch == '+' {
            out.push_str(" and ");
        } else if ch.is_alphanumeric() || ch.is_whitespace() {
            out.push(ch);
        } else if ch != '\'' && ch != '’' {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_artist(raw: &str) -> String {
    let folded = nfkc_fold(raw.trim());
    let head = folded
        .rsplit_once('-')
        .filter(|(_, tail)| tail.trim().eq_ignore_ascii_case("topic"))
        .map_or(folded.as_str(), |(h, _)| h.trim_end());
    strip_feat_clause(head).trim().to_string()
}

pub(crate) fn normalized_track_parts(track: &Track) -> NormalizedTrackParts {
    let pri_clean = clean_artist(track.primary_artist_name().unwrap_or_default());
    let pri_norm = normalize_text_core(&pri_clean);
    let title_folded = nfkc_fold(track.title.trim());
    let split = [" - ", " – ", " — "].into_iter().find_map(|sep| {
        let (l, r) = title_folded.split_once(sep)?;
        (!l.trim().is_empty() && !r.trim().is_empty()).then_some((l.trim(), r.trim()))
    });

    let (artist_display, artist_norm, title_raw) = match split {
        Some((left, right)) => {
            let left_clean = clean_artist(left);
            let left_norm = normalize_text_core(&left_clean);
            let is_ugc = track.music_video_type == MusicVideoType::UserGenerated;
            if pri_norm.is_empty() || left_norm == pri_norm || (is_ugc && !is_variant_suffix(right))
            {
                (left_clean, left_norm, right)
            } else {
                (pri_clean, pri_norm, title_folded.as_str())
            }
        }
        None => (pri_clean, pri_norm, title_folded.as_str()),
    };

    let no_brackets = strip_bracket_noise(title_raw);
    let title_display = strip_feat_clause(strip_trailing_noise(no_brackets.trim()))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title_norm = normalize_text_core(&title_display);
    let cache_key = format!("{artist_norm}::{title_norm}");

    NormalizedTrackParts {
        cache_key,
        artist_norm,
        title_norm,
        artist_display,
        title_display,
    }
}

/// Builds the normalized `(cache_key, artist_norm, title_norm)` tuple for a track.
#[must_use]
pub fn build_cache_key(track: &Track) -> (String, String, String) {
    let parts = normalized_track_parts(track);
    (parts.cache_key, parts.artist_norm, parts.title_norm)
}

#[cfg(test)]
mod tests {
    use ytm_core::{ArtistRef, TrackAvailability};

    use super::*;

    fn mk_track(artist: &str, title: &str, kind: MusicVideoType) -> Track {
        Track {
            video_id: "V1".to_string(),
            set_video_id: None,
            position: 0,
            title: title.to_string(),
            artists: vec![ArtistRef {
                name: artist.to_string(),
                browse_id: None,
            }],
            album: None,
            duration_sec: None,
            release_year: None,
            music_video_type: kind,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    #[test]
    fn strips_noise_nfkc_and_topic_suffix() {
        let track = mk_track(
            "Daft Punk - Topic",
            "Ａｒｏｕｎｄ　ｔｈｅ　Ｗｏｒｌｄ (Official Video) [Lyrics] (2011 Remastered) feat. Pharrell",
            MusicVideoType::AudioTrack,
        );
        let (key, artist, title) = build_cache_key(&track);
        assert_eq!(artist, "daft punk");
        assert_eq!(title, "around the world");
        assert_eq!(key, "daft punk::around the world");
    }

    #[test]
    fn ugc_artist_title_split_respects_variant_suffix_guard() {
        let ugc_split = mk_track(
            "Uploader99",
            "Kavinsky - Nightcall (Official Audio)",
            MusicVideoType::UserGenerated,
        );
        assert_eq!(
            build_cache_key(&ugc_split).0,
            "kavinsky::nightcall".to_string()
        );

        let ugc_remix = mk_track(
            "Daft Punk",
            "Get Lucky - Remix",
            MusicVideoType::UserGenerated,
        );
        assert_eq!(
            build_cache_key(&ugc_remix).0,
            "daft punk::get lucky remix".to_string()
        );
    }

    #[test]
    fn composes_decomposed_latin_accents_like_nfkc() {
        let nfd = mk_track("Cafe\u{0301}", "ﬁne", MusicVideoType::AudioTrack);
        let nfc = mk_track("Café", "fine", MusicVideoType::AudioTrack);
        assert_eq!(build_cache_key(&nfd), build_cache_key(&nfc));
    }
}
