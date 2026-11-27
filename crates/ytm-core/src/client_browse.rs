use serde_json::json;

use crate::browse_models::{BrowsePage, LyricsPayload, SearchFilter, SearchPage};
use crate::client::InnerTubeClient;
use crate::error::CoreError;
use crate::http::RetryPolicy;
use crate::parser::{
    check_innertube_error, parse_browse_continuation, parse_browse_page, parse_lyrics_payload,
    parse_search_page, parse_search_suggestions, to_browse_playlist_id,
};
use crate::traits::HttpTransport;

pub(crate) fn normalize_browse_target(browse_id: &str) -> String {
    let trimmed = browse_id.trim();
    if trimmed.eq_ignore_ascii_case("LM")
        || trimmed.starts_with("PL")
        || trimmed.starts_with("OLAK")
        || trimmed.starts_with("RD")
        || trimmed.starts_with("UU")
        || trimmed.starts_with("FL")
        || trimmed.starts_with("LL")
    {
        to_browse_playlist_id(trimmed)
    } else {
        trimmed.to_string()
    }
}

impl<T: HttpTransport> InnerTubeClient<T> {
    pub(crate) async fn get_browse_page_impl(
        &self,
        browse_id: &str,
        params: Option<&str>,
    ) -> Result<BrowsePage, CoreError> {
        let normalized = normalize_browse_target(browse_id);
        let mut body = json!({ "browseId": normalized });
        if let Some(p) = params
            && !p.trim().is_empty()
        {
            body.as_object_mut()
                .expect("object")
                .insert("params".to_string(), json!(p.trim()));
        }
        let resp = self
            .post_authed_with_policy("browse", body, RetryPolicy::IdempotentRead)
            .await?;
        parse_browse_page(&resp, &normalized)
    }
    pub(crate) async fn get_search_suggestions_impl(
        &self,
        query: &str,
    ) -> Result<Vec<String>, CoreError> {
        let resp = self
            .post_authed_with_policy(
                "music/get_search_suggestions",
                json!({ "input": query }),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        parse_search_suggestions(&resp)
    }
    pub(crate) async fn search_catalog_impl(
        &self,
        query: &str,
        filter: SearchFilter,
        continuation: Option<&str>,
    ) -> Result<SearchPage, CoreError> {
        let body = if let Some(token) = continuation {
            json!({ "continuation": token })
        } else {
            let mut obj = serde_json::Map::new();
            obj.insert("query".to_string(), json!(query));
            if let Some(params) = filter.innertube_params() {
                obj.insert("params".to_string(), json!(params));
            }
            serde_json::Value::Object(obj)
        };
        let resp = self
            .post_authed_with_policy("search", body, RetryPolicy::IdempotentRead)
            .await?;
        parse_search_page(&resp)
    }

    pub(crate) async fn get_browse_continuation_impl(
        &self,
        token: &str,
    ) -> Result<BrowsePage, CoreError> {
        let resp = self
            .post_authed_with_policy(
                "browse",
                json!({ "continuation": token }),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        parse_browse_continuation(&resp)
    }

    pub(crate) async fn get_lyrics_payload_impl(
        &self,
        lyrics_browse_id: &str,
    ) -> Result<Option<LyricsPayload>, CoreError> {
        let resp = self
            .post_authed_with_policy(
                "browse",
                json!({ "browseId": lyrics_browse_id }),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        check_innertube_error(&resp)?;
        Ok(parse_lyrics_payload(&resp))
    }
}
