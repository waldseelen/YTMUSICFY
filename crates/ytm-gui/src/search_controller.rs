use std::collections::VecDeque;

use ytm_core::{BrowseCard, BrowseCardKind, SearchPage, Track};

pub const SUGGESTION_DEBOUNCE_MS: u64 = 100;
pub const LIVE_SEARCH_DEBOUNCE_MS: u64 = 250;
pub const MAX_RECENT_SEARCHES: usize = 12;

/// Network requests due after debounce timers elapse.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SearchDueActions {
    pub fetch_suggestions: Option<(u64, String)>,
    pub fetch_live: Option<(u64, String)>,
}

/// Debounced instant search state machine with monotonic request IDs and LRU query history.
#[derive(Debug, Clone, Default)]
pub struct SearchController {
    pub query: String,
    pub last_edit_ms: u64,
    suggestion_sent: bool,
    live_sent: bool,
    next_request_id: u64,
    active_suggestion_req: u64,
    active_live_req: u64,
    suggestion_sent_at_ms: u64,
    pub last_suggestion_latency_ms: Option<u64>,
    pub suggestions: Vec<String>,
    pub live_page: Option<SearchPage>,
    pub recent_queries: VecDeque<String>,
    pub dropdown_open: bool,
    pub highlighted_idx: Option<usize>,
}

impl SearchController {
    pub fn load_recent(&mut self, recent: &[String]) {
        self.recent_queries.clear();
        for q in recent.iter().take(MAX_RECENT_SEARCHES) {
            let trimmed = q.trim();
            if !trimmed.is_empty() {
                self.recent_queries.push_back(trimmed.to_string());
            }
        }
    }

    /// Record an edit to the global search input, resetting debounce windows.
    pub fn on_query_change(&mut self, next: &str, now_ms: u64) {
        if self.query == next {
            return;
        }
        self.query = next.to_string();
        self.last_edit_ms = now_ms;
        self.highlighted_idx = None;
        self.dropdown_open = true;
        // Invalidate any in-flight responses immediately on any edit.
        self.next_request_id = self.next_request_id.saturating_add(1);
        self.active_suggestion_req = self.next_request_id;
        self.active_live_req = self.next_request_id;
        if next.trim().is_empty() {
            self.suggestion_sent = true;
            self.live_sent = true;
            self.suggestions.clear();
            self.live_page = None;
        } else {
            self.suggestion_sent = false;
            self.live_sent = false;
        }
    }

    /// Check debounce deadlines at `now_ms` and return any due suggestion/live requests.
    pub fn poll_due(&mut self, now_ms: u64) -> SearchDueActions {
        let mut out = SearchDueActions::default();
        let trimmed = self.query.trim();
        if trimmed.is_empty() {
            return out;
        }
        let elapsed = now_ms.saturating_sub(self.last_edit_ms);
        if !self.suggestion_sent && elapsed >= SUGGESTION_DEBOUNCE_MS {
            self.suggestion_sent = true;
            self.next_request_id = self.next_request_id.saturating_add(1);
            self.active_suggestion_req = self.next_request_id;
            self.suggestion_sent_at_ms = now_ms;
            out.fetch_suggestions = Some((self.active_suggestion_req, trimmed.to_string()));
        }
        if !self.live_sent && elapsed >= LIVE_SEARCH_DEBOUNCE_MS {
            self.live_sent = true;
            self.next_request_id = self.next_request_id.saturating_add(1);
            self.active_live_req = self.next_request_id;
            out.fetch_live = Some((self.active_live_req, trimmed.to_string()));
        }
        out
    }

    /// Apply suggestion items only when `req_id` matches the latest active suggestion request.
    pub fn apply_suggestions(&mut self, req_id: u64, items: Vec<String>, now_ms: u64) -> bool {
        if req_id != self.active_suggestion_req {
            return false;
        }
        self.last_suggestion_latency_ms = Some(now_ms.saturating_sub(self.suggestion_sent_at_ms));
        self.suggestions = items;
        true
    }

    /// Apply live results only when `req_id` matches the latest active live search request.
    pub fn apply_live_results(&mut self, req_id: u64, page: SearchPage) -> bool {
        if req_id != self.active_live_req {
            return false;
        }
        self.live_page = Some(page);
        true
    }

    /// Insert `query` at the front of the bounded LRU recent search list.
    pub fn record_recent(&mut self, query: &str) -> Vec<String> {
        let trimmed = query.trim();
        if !trimmed.is_empty() {
            self.recent_queries
                .retain(|q| !q.eq_ignore_ascii_case(trimmed));
            self.recent_queries.push_front(trimmed.to_string());
            while self.recent_queries.len() > MAX_RECENT_SEARCHES {
                self.recent_queries.pop_back();
            }
        }
        self.recent_queries.iter().cloned().collect()
    }

    /// Extract structured groups (`TopMatch`, up to 3 `Songs`, `Artist`, `Album`) from live results.
    #[must_use]
    pub fn grouped_preview(&self) -> GroupedSearchPreview<'_> {
        let Some(page) = &self.live_page else {
            return GroupedSearchPreview::default();
        };
        let top_card = page.cards.first();
        let top_track = if top_card.is_none() {
            page.tracks.first()
        } else {
            None
        };
        let songs: Vec<&Track> = page.tracks.iter().take(3).collect();
        let artist = page.cards.iter().find(|c| c.kind == BrowseCardKind::Artist);
        let album = page.cards.iter().find(|c| c.kind == BrowseCardKind::Album);
        GroupedSearchPreview {
            top_card,
            top_track,
            songs,
            artist,
            album,
        }
    }
}

#[derive(Debug, Default)]
pub struct GroupedSearchPreview<'a> {
    pub top_card: Option<&'a BrowseCard>,
    pub top_track: Option<&'a Track>,
    pub songs: Vec<&'a Track>,
    pub artist: Option<&'a BrowseCard>,
    pub album: Option<&'a BrowseCard>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_coalesces_keystrokes_and_ignores_stale_responses() {
        let mut ctrl = SearchController::default();

        ctrl.on_query_change("d", 100);
        assert_eq!(ctrl.poll_due(150), SearchDueActions::default());

        // Rapid next keystroke resets the debounce timer.
        ctrl.on_query_change("daft", 160);
        assert_eq!(ctrl.poll_due(240), SearchDueActions::default());

        // At +100ms (260ms) suggestions fire, live search does not yet.
        let due1 = ctrl.poll_due(260);
        let (sug_id_1, sug_q) = due1.fetch_suggestions.expect("suggestions due at 100ms");
        assert_eq!(sug_q, "daft");
        assert!(due1.fetch_live.is_none());

        // User keeps typing -> invalidates old suggestion request ID.
        ctrl.on_query_change("daft punk", 300);
        assert!(!ctrl.apply_suggestions(sug_id_1, vec!["stale".to_string()], 310));
        assert!(ctrl.suggestions.is_empty());

        // At +250ms (550ms) both new suggestions and live results are due.
        let due2 = ctrl.poll_due(550);
        let (sug_id_2, q2) = due2.fetch_suggestions.expect("suggestions 2");
        let (live_id_2, lq2) = due2.fetch_live.expect("live 2");
        assert_eq!(q2, "daft punk");
        assert_eq!(lq2, "daft punk");

        assert!(ctrl.apply_suggestions(sug_id_2, vec!["daft punk".to_string()], 580));
        assert_eq!(ctrl.suggestions, vec!["daft punk"]);
        assert_eq!(ctrl.last_suggestion_latency_ms, Some(30));

        assert!(!ctrl.apply_live_results(
            sug_id_1,
            SearchPage {
                tracks: Vec::new(),
                cards: Vec::new(),
                continuation: None,
            },
        ));
        assert!(ctrl.live_page.is_none());

        assert!(ctrl.apply_live_results(
            live_id_2,
            SearchPage {
                tracks: Vec::new(),
                cards: Vec::new(),
                continuation: None,
            },
        ));
        assert!(ctrl.live_page.is_some());
    }

    #[test]
    fn recent_queries_lru_deduplicates_and_caps() {
        let mut ctrl = SearchController::default();
        for i in 0..15 {
            ctrl.record_recent(&format!("query-{i}"));
        }
        assert_eq!(ctrl.recent_queries.len(), MAX_RECENT_SEARCHES);
        assert_eq!(ctrl.recent_queries.front().unwrap(), "query-14");

        ctrl.record_recent("QUERY-5");
        assert_eq!(ctrl.recent_queries.front().unwrap(), "QUERY-5");
        assert_eq!(ctrl.recent_queries.len(), MAX_RECENT_SEARCHES);
    }
}
