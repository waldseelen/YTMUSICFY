use serde_json::Value;

use crate::models::ArtistRef;
use crate::parser::tree::{extract_runs_text, find_first_by_key};

pub(crate) fn extract_header_artists(header: Option<&Value>) -> Vec<ArtistRef> {
    let Some(hdr) = header else {
        return Vec::new();
    };
    let mut artists = Vec::new();
    if let Some(runs) = hdr
        .get("straplineTextOne")
        .and_then(|s| s.get("runs"))
        .and_then(Value::as_array)
    {
        for run in runs {
            let txt = run.get("text").and_then(Value::as_str).unwrap_or("").trim();
            if txt.is_empty() || matches!(txt, "•" | "&" | "," | "/") {
                continue;
            }
            let bid = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(Value::as_str)
                .map(str::to_string);
            artists.push(ArtistRef {
                name: txt.to_string(),
                browse_id: bid,
            });
        }
    }
    if artists.is_empty()
        && let Some(facepile) = hdr.get("facepile")
    {
        let name = facepile
            .pointer("/avatarStackViewModel/text")
            .map(extract_runs_text)
            .unwrap_or_default();
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            let bid = find_first_by_key(facepile, "browseEndpoint")
                .and_then(|ep| ep.get("browseId"))
                .and_then(Value::as_str)
                .map(str::to_string);
            artists.push(ArtistRef {
                name: trimmed.to_string(),
                browse_id: bid,
            });
        }
    }
    artists
}

pub(crate) fn combine_subtitle(header: Option<&Value>, response: &Value) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for val in [
        header.and_then(|h| h.get("subtitle")),
        header.and_then(|h| h.get("secondSubtitle")),
    ]
    .into_iter()
    .flatten()
    {
        let txt = extract_runs_text(val).trim().to_string();
        if !txt.is_empty() {
            parts.push(txt);
        }
    }
    if parts.is_empty()
        && let Some(desc) = header
            .and_then(|h| h.get("description"))
            .and_then(|d| {
                find_first_by_key(d, "description")
                    .or(Some(d))
                    .map(extract_runs_text)
            })
            .or_else(|| {
                find_first_by_key(response, "musicDescriptionShelfRenderer")
                    .and_then(|d| d.get("description"))
                    .map(extract_runs_text)
            })
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    {
        parts.push(desc);
    }
    (!parts.is_empty()).then(|| parts.join(" • "))
}
