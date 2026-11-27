use std::fmt::Write as _;

/// Percent-encode a string for URL query parameters (UTF-8 safe).
#[must_use]
pub fn encode_query_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push('+'),
            b => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Build a YouTube Music track URL, optionally starting at `at_sec` (`&t=`).
#[must_use]
pub fn ytm_track_url(video_id: &str, at_sec: Option<u64>) -> String {
    let vid = encode_query_component(video_id.trim());
    match at_sec.filter(|&s| s > 0) {
        Some(sec) => format!("https://music.youtube.com/watch?v={vid}&t={sec}"),
        None => format!("https://music.youtube.com/watch?v={vid}"),
    }
}

/// Build a YouTube Music album browse URL.
#[must_use]
pub fn ytm_album_url(browse_id: &str) -> String {
    format!(
        "https://music.youtube.com/browse/{}",
        encode_query_component(browse_id.trim())
    )
}

/// Build a YouTube Music artist channel or browse URL.
#[must_use]
pub fn ytm_artist_url(browse_id: &str) -> String {
    let trimmed = browse_id.trim();
    if trimmed.starts_with("UC") {
        format!(
            "https://music.youtube.com/channel/{}",
            encode_query_component(trimmed)
        )
    } else {
        format!(
            "https://music.youtube.com/browse/{}",
            encode_query_component(trimmed)
        )
    }
}

/// Build a YouTube Music playlist URL (normalizing `"VL"` browse prefixes).
#[must_use]
pub fn ytm_playlist_url(playlist_id: &str) -> String {
    let trimmed = playlist_id.trim();
    let list_id = if trimmed.eq_ignore_ascii_case("VLLM") {
        "LM"
    } else {
        trimmed.strip_prefix("VL").unwrap_or(trimmed)
    };
    format!(
        "https://music.youtube.com/playlist?list={}",
        encode_query_component(list_id)
    )
}

/// Build a standard YouTube search URL (`https://www.youtube.com/results?search_query=...`).
#[must_use]
pub fn youtube_search_url(query: &str) -> String {
    format!(
        "https://www.youtube.com/results?search_query={}",
        encode_query_component(query.trim())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_builders_handle_edge_cases_and_utf8_encoding() {
        assert_eq!(
            ytm_track_url("abc_123", None),
            "https://music.youtube.com/watch?v=abc_123"
        );
        assert_eq!(
            ytm_track_url("abc_123", Some(0)),
            "https://music.youtube.com/watch?v=abc_123"
        );
        assert_eq!(
            ytm_track_url("abc_123", Some(95)),
            "https://music.youtube.com/watch?v=abc_123&t=95"
        );
        assert_eq!(
            ytm_album_url("MPREb_xyz"),
            "https://music.youtube.com/browse/MPREb_xyz"
        );
        assert_eq!(
            ytm_artist_url("UC12345"),
            "https://music.youtube.com/channel/UC12345"
        );
        assert_eq!(
            ytm_artist_url("MPLA_99"),
            "https://music.youtube.com/browse/MPLA_99"
        );
        assert_eq!(
            ytm_playlist_url("VLPLabc"),
            "https://music.youtube.com/playlist?list=PLabc"
        );
        assert_eq!(
            ytm_playlist_url("VLLM"),
            "https://music.youtube.com/playlist?list=LM"
        );

        let search_cases = [
            (
                "Daft Punk & Pharrell?",
                "https://www.youtube.com/results?search_query=Daft+Punk+%26+Pharrell%3F",
            ),
            (
                "Şebnem Ferah #1+1",
                "https://www.youtube.com/results?search_query=%C5%9Eebnem+Ferah+%231%2B1",
            ),
        ];
        for (input, expected) in search_cases {
            assert_eq!(youtube_search_url(input), expected);
        }
    }
}
