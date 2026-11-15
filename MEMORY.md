# MEMORY.md

Durable, non-obvious protocol traps and systems invariants verified against real YouTube Music InnerTube behavior.

## Trap 1: Logged-Out HTTP `200 OK` Silent Fallback
- InnerTube frequently returns `HTTP 200 OK` with an empty or anonymous/logged-out payload when cookies expire, instead of `401` or `403`.
- Never treat `status == 200` on `browse` as authenticated. Always verify explicit signed-in markers via `verify_session()` (`account/account_menu` header / active account email/channel handle or owned editable library markers) before trusting session state.

## Trap 2: Dual `continuationItemRenderer` vs Legacy `continuations` Pagination
- Modern playlist browse responses (`get_playlist`) append a `continuationItemRenderer` as the final item inside `musicPlaylistShelfRenderer.contents`, with the token either at `continuationEndpoint.continuationCommand.token` or inside `commandExecutorCommand.commands[]` where `continuationCommand.request == "CONTINUATION_REQUEST_TYPE_BROWSE"`.
- Continuation requests send `POST browse` with JSON body `{"context": ..., "continuation": token}` (not query-only `ctoken`/`continuation` params), and items return under `onResponseReceivedActions[0].appendContinuationItemsAction.continuationItems`.
- Library playlists and older shelves may still use `continuations[0].nextContinuationData` + `continuationContents`. Parsers must handle both without failing on missing intermediate wrappers.

## Trap 3: `ACTION_REMOVE_VIDEO` Requires Both `setVideoId` and `removedVideoId`
- Playlist items can share identical `videoId` values (exact duplicates). InnerTube distinguishes playlist instances by `setVideoId`.
- `ACTION_REMOVE_VIDEO` payloads must pass both `"setVideoId"` and `"removedVideoId"`. Furthermore, non-owned playlists and `"VLLM"` (`LM`) omit `setVideoId` (items cannot be deleted via `browse/edit_playlist`), so mutations must detect missing `setVideoId` upfront and fail with `CoreError::MissingSetVideoId`.

## Trap 4: `ACTION_MOVE_VIDEO_BEFORE` and Moving to End
- Moving a track before another requires `{"action": "ACTION_MOVE_VIDEO_BEFORE", "setVideoId": item_id, "movedSetVideoIdSuccessor": target_successor_id}`.
- To move an item to the very end of the playlist, omit `movedSetVideoIdSuccessor` entirely (do not pass a dummy ID). Every move mutates live indices immediately, so sequential minimal-move actions calculated via LIS must preserve exact generation order.

## Trap 5: Brand Accounts Require Both Header and Context
- Multi-account Google sessions need `X-Goog-AuthUser` (typically `"0"`, `"1"`, ...).
- Secondary YouTube channels / Brand Accounts require both the `X-Goog-PageId` HTTP header and `context.user.onBehalfOfUser` in the JSON body (both carrying the Brand Account ID).

## Trap 6: `__Secure-3PAPISID` Cookie Priority for `SAPISIDHASH`
- Browser cookie jars on `music.youtube.com` may carry `__Secure-3PAPISID` without a bare `SAPISID` cookie, or both. Check `__Secure-3PAPISID` and `SAPISID`, strip surrounding quotes if present, and compute `SHA1("{timestamp} {sapisid} https://music.youtube.com")` matching `Origin: https://music.youtube.com`.

## Trap 7: `UGC` Tracks Embed `"Artist - Title"` in Title
- User-uploaded YouTube videos (`MUSIC_VIDEO_TYPE_UGC`) often report the uploader channel as `artists[0]` while placing the real `"Artist - Title"` string inside `title`. Fuzzy deduplication and normalization must split `UGC` hyphenated titles carefully without corrupting genuinely hyphenated `ATV` album titles.

## Trap 8: Empty `ACTION_ADD_VIDEO` Sentinel & `DEDUPE_OPTION_SKIP`
- `ACTION_ADD_VIDEO` rejects duplicate additions unless `"dedupeOption": "DEDUPE_OPTION_SKIP"` is included on the action object.
- When copying items from another playlist via `ACTION_ADD_PLAYLIST`, InnerTube requires a trailing `{"action": "ACTION_ADD_VIDEO", "addedVideoId": null}` if `playlistEditResults` mapping new `setVideoId`s is needed.

## Trap 9: `context.user` Must Be Sibling of `context.client` & `playlistItemData.playlistSetVideoId`
- `context.user` (holding `onBehalfOfUser` for Brand Accounts) MUST be a sibling of `context.client` directly under `context`, never nested inside `context.client`.
- Greyed-out unavailable tracks omit `menu` (and `playlistEditEndpoint`), yet still populate `playlistItemData.playlistSetVideoId` and `playlistItemData.videoId`. Parsers must read `playlistItemData` first so dead tracks can still be removed via `ACTION_REMOVE_VIDEO`.

## Trap 10: HTTP/2 Header Size & Fresh-Playlist Creation Race
- `music.youtube.com` response headers can exceed hyper's default 16 KiB HTTP/2 header list cap; configure `http2_max_header_list_size(64 * 1024)` (or HTTP/1.1 fallback) on the `reqwest` client.
- Calling `browse/edit_playlist` immediately after `playlist/create` can transiently return HTTP `409` or `400` for several seconds; apply retry with backoff after playlist creation.

## Trap 11: Cargo 1.100 Default-On `unused_workspace_dependencies` & `unused_dependencies`
- Cargo 1.100 enables `cargo::unused_workspace_dependencies` and `cargo::unused_dependencies` by default.
- In contract-first multi-wave workspaces where root `Cargo.toml` pins Wave 1–3 dependencies ahead of implementation, link deferred crates via `use <crate> as _;` inside each crate's `src/lib.rs` until real modules import them (rather than adding lint suppressions).
