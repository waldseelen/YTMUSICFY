use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary};
use ytm_enrich::{GenreBucket, GenreSplitPlan};

use crate::navigation::ToastState;
use crate::state::{AsyncState, ConfirmedMutation, GenreSplitForm, PendingMutation, WorkerCmd};
use crate::sync_queue::{StagedEntry, SyncQueue};
use crate::views::async_banner;
use crate::views::components::{
    hash_form, load_plan_fingerprint, make_fingerprint, store_plan_fingerprint,
};

#[path = "genre_split_ops.rs"]
mod ops;
pub use ops::build_genre_split_ops;

const GENRE_FP_SALT: &str = "genre_split_plan_fp";

/// Parameters bundled for `show_genre_split` to keep call sites concise.
pub struct GenreSplitViewCtx<'a> {
    pub playlists: &'a [PlaylistSummary],
    pub selected_playlist_id: &'a mut Option<String>,
    pub form: &'a mut GenreSplitForm,
    pub plan_state: &'a mut AsyncState<(Playlist, GenreSplitPlan)>,
    pub batch_progress: Option<&'a (usize, usize, String)>,
    pub sync_queue: &'a mut SyncQueue,
    pub pending_modal: &'a mut Option<PendingMutation>,
    pub toast: &'a mut Option<ToastState>,
    pub cmd_tx: &'a Sender<WorkerCmd>,
}

pub fn show_genre_split(ui: &mut egui::Ui, ctx: GenreSplitViewCtx<'_>) {
    let GenreSplitViewCtx {
        playlists,
        selected_playlist_id,
        form,
        plan_state,
        batch_progress,
        sync_queue,
        pending_modal,
        toast,
        cmd_tx,
    } = ctx;

    let selected_sum = selected_playlist_id
        .as_deref()
        .and_then(|pid| playlists.iter().find(|p| p.id == pid));
    let combo_label = selected_sum.map_or_else(
        || "<select source playlist>".to_string(),
        |p| p.title.clone(),
    );

    ui.horizontal_wrapped(|ui| {
        ui.strong("Source Playlist:");
        egui::ComboBox::from_id_salt("genre_split_source_combo")
            .selected_text(combo_label)
            .show_ui(ui, |ui| {
                for p in playlists {
                    let count = p.track_count.map_or("?".into(), |n| n.to_string());
                    ui.selectable_value(
                        selected_playlist_id,
                        Some(p.id.clone()),
                        format!("{} ({count})", p.title),
                    );
                }
            });
        ui.label("Min tracks/genre:");
        ui.add(egui::DragValue::new(&mut form.min_tracks).range(1..=100));
        for preset in [1usize, 3, 5] {
            if ui.small_button(format!("{preset}")).clicked() {
                form.min_tracks = preset;
            }
        }
        ui.label("Custom genres.toml:");
        ui.add(
            egui::TextEdit::singleline(&mut form.custom_genres_toml)
                .desired_width(170.0)
                .hint_text("Optional custom TOML path"),
        );
    });

    ui.horizontal_wrapped(|ui| {
        let can_move_source = selected_sum
            .is_some_and(|p| p.owned && !p.id.eq_ignore_ascii_case("LM"));
        if !can_move_source {
            form.move_mode = false;
        }
        ui.radio_value(&mut form.move_mode, false, "Copy into genre playlists")
            .on_hover_text("Create/append genre playlists while keeping the source playlist intact");
        ui.add_enabled(
            can_move_source,
            egui::RadioButton::new(form.move_mode, "Split & Move (remove from source)"),
        )
        .on_hover_text("Create/append genre playlists, then remove transferred tracks from source (pre-snapshot saved automatically)")
        .clicked()
        .then(|| form.move_mode = true);
        ui.checkbox(&mut form.include_skipped, "Show & allow small buckets");
    });

    let form_sig = hash_form(&(form.min_tracks, &form.custom_genres_toml));
    let sel_id = selected_playlist_id.as_deref();
    let current_fp = sel_id.map(|pid| make_fingerprint(pid, sync_queue.revision(), form_sig));
    let plan_is_fresh = current_fp.is_some()
        && load_plan_fingerprint(ui.ctx(), GENRE_FP_SALT) == current_fp
        && plan_state
            .data()
            .is_some_and(|(pl, _)| Some(pl.id.as_str()) == sel_id);

    ui.horizontal_wrapped(|ui| {
        let can_plan = sel_id.is_some() && !plan_state.is_loading();
        if ui
            .add_enabled(can_plan, egui::Button::new("Analyze & Preview Split"))
            .clicked()
            && let Some(pid) = sel_id
        {
            form.excluded_buckets.clear();
            form.excluded_video_ids.clear();
            if let Some(fp) = &current_fp {
                store_plan_fingerprint(ui.ctx(), GENRE_FP_SALT, fp.clone());
            }
            *plan_state = AsyncState::Loading(
                "Enriching tracks (SQLite cache + MusicBrainz / Deezer)...".to_string(),
            );
            let custom = (!form.custom_genres_toml.trim().is_empty())
                .then(|| PathBuf::from(form.custom_genres_toml.trim()));
            let _ = cmd_tx.send(WorkerCmd::PlanGenreSplit {
                playlist_id: pid.to_string(),
                min_tracks: form.min_tracks,
                custom_genres_path: custom,
            });
        }

        if let Some((source_pl, plan)) = plan_state.data() {
            let (ops, bucket_cnt, track_cnt) = build_genre_split_ops(source_pl, plan, form);
            if bucket_cnt > 0 {
                if ui
                    .add_enabled(
                        plan_is_fresh,
                        egui::Button::new(format!(
                            "Split & Apply Now ({bucket_cnt} list, {track_cnt} tracks)..."
                        )),
                    )
                    .clicked()
                {
                    let entries: Vec<StagedEntry> = ops
                        .iter()
                        .enumerate()
                        .map(|(i, op)| StagedEntry {
                            id: 9_000_000_000 + i as u64,
                            op: op.clone(),
                            selected: true,
                        })
                        .collect();
                    let verb = if form.move_mode { "Split & Move" } else { "Split (Copy)" };
                    *pending_modal = Some(PendingMutation {
                        title: format!("Confirm {verb}: {}", source_pl.title),
                        summary: format!(
                            "Save mandatory pre-sync SQLite snapshot(s) and execute {bucket_cnt} genre playlist operation(s) covering {track_cnt} track(s) directly to YouTube Music?"
                        ),
                        action: ConfirmedMutation::ExecuteSync { entries },
                    });
                }
                if ui
                    .add_enabled(plan_is_fresh, egui::Button::new("Stage to Sync"))
                    .clicked()
                {
                    let mut last_id = None;
                    for op in ops {
                        last_id = Some(sync_queue.push(op));
                    }
                    *toast = Some(ToastState::staged(
                        format!("Staged {bucket_cnt} genre bucket(s) in Sync."),
                        last_id,
                    ));
                }
                if !plan_is_fresh {
                    ui.colored_label(
                        egui::Color32::from_rgb(235, 180, 70),
                        "Playlist or parameters changed — re-run Analyze to apply.",
                    );
                }
            }
        }
    });

    if plan_state.is_loading()
        && let Some((done, total, msg)) = batch_progress
    {
        let frac = (*done as f32 / (*total).max(1) as f32).clamp(0.0, 1.0);
        ui.add(egui::ProgressBar::new(frac).text(msg.as_str()));
    }
    let (loading, error) = match plan_state {
        AsyncState::Loading(m) => (Some(m.as_str()), None),
        AsyncState::Failed(e) => (None, Some(e.as_str())),
        _ => (None, None),
    };
    async_banner(ui, loading, error);
    ui.separator();

    let Some((_, plan)) = plan_state.data() else {
        ui.label("Choose a source playlist and click Analyze & Preview Split to inspect genre buckets and divide directly.");
        return;
    };

    ui.label(format!(
        "Buckets: {} | Small (< {} tracks): {} | Enriched tracks: {}",
        plan.buckets.len(),
        form.min_tracks,
        plan.skipped_small_buckets,
        plan.track_details.len()
    ));
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("genre_split_buckets_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_bucket_list(ui, &plan.buckets, plan, form, false);
            if form.include_skipped && !plan.skipped_buckets.is_empty() {
                ui.add_space(8.0);
                ui.strong(format!("Small Buckets (< {} tracks)", form.min_tracks));
                render_bucket_list(ui, &plan.skipped_buckets, plan, form, true);
            }
        });
}

fn render_bucket_list(
    ui: &mut egui::Ui,
    buckets: &[GenreBucket],
    plan: &GenreSplitPlan,
    form: &mut GenreSplitForm,
    is_small: bool,
) {
    for bucket in buckets {
        let mut included = !form.excluded_buckets.contains(&bucket.genre);
        let expanded = form.expanded_buckets.contains(&bucket.genre);
        let arrow = if expanded { "v" } else { ">" };
        let action = if bucket.existing_playlist_id.is_some() {
            "APPEND"
        } else {
            "CREATE"
        };
        let active_cnt = bucket
            .tracks_to_add
            .iter()
            .filter(|t| !form.excluded_video_ids.contains(&t.video_id))
            .count();

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.checkbox(&mut included, "").changed() {
                    if included {
                        form.excluded_buckets.remove(&bucket.genre);
                    } else {
                        form.excluded_buckets.insert(bucket.genre.clone());
                    }
                }
                if ui.small_button(arrow).clicked() {
                    if expanded {
                        form.expanded_buckets.remove(&bucket.genre);
                    } else {
                        form.expanded_buckets.insert(bucket.genre.clone());
                    }
                }
                ui.strong(&bucket.genre);
                ui.label("->");
                ui.monospace(&bucket.target_playlist_title);
                ui.small(format!(
                    "[{action} · +{active_cnt} new · {} existing{}]",
                    bucket.already_present_count,
                    if is_small { " · small" } else { "" }
                ));
            });

            if expanded {
                ui.separator();
                if bucket.tracks_to_add.is_empty() {
                    ui.weak("All tracks in this genre are already present in the target playlist.");
                } else {
                    for track in &bucket.tracks_to_add {
                        let mut track_on = !form.excluded_video_ids.contains(&track.video_id);
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(18.0);
                            if ui.checkbox(&mut track_on, "").changed() {
                                if track_on {
                                    form.excluded_video_ids.remove(&track.video_id);
                                } else {
                                    form.excluded_video_ids.insert(track.video_id.clone());
                                }
                            }
                            ui.label(format!(
                                "{} — {}",
                                track.title,
                                track.primary_artist_name().unwrap_or("-")
                            ));
                            if let Some(info) = plan.track_details.get(&track.video_id) {
                                let tags = if info.raw_tags.is_empty() {
                                    "no provider tags".to_string()
                                } else {
                                    info.raw_tags
                                        .iter()
                                        .take(3)
                                        .cloned()
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                };
                                ui.weak(format!("({}: {tags})", info.source.as_str()));
                            }
                        });
                    }
                }
            }
        });
        ui.add_space(4.0);
    }
}
