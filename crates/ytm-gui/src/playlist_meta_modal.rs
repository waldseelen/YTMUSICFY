use eframe::egui;
use ytm_core::PlaylistPrivacy;

use crate::navigation::ToastState;
use crate::sync_queue::{StagedOp, SyncQueue};

/// Form state for editing an owned playlist's title, description, and privacy.
#[derive(Debug, Clone)]
pub struct PlaylistMetaModal {
    pub playlist_id: String,
    pub original_title: String,
    pub original_description: String,
    pub original_privacy: PlaylistPrivacy,
    pub title: String,
    pub description: String,
    pub privacy: PlaylistPrivacy,
}

impl PlaylistMetaModal {
    #[must_use]
    pub fn new(
        playlist_id: String,
        title: String,
        description: String,
        privacy: PlaylistPrivacy,
    ) -> Self {
        Self {
            playlist_id,
            original_title: title.clone(),
            original_description: description.clone(),
            original_privacy: privacy,
            title,
            description,
            privacy,
        }
    }

    /// Computes a sparse metadata update op containing only fields that changed.
    #[must_use]
    pub fn to_staged_op(&self) -> Option<StagedOp> {
        let trimmed_title = self.title.trim();
        let title =
            (trimmed_title != self.original_title.trim()).then(|| trimmed_title.to_string());
        let description =
            (self.description != self.original_description).then(|| self.description.clone());
        let privacy = (self.privacy != self.original_privacy).then_some(self.privacy);
        if title.is_none() && description.is_none() && privacy.is_none() {
            return None;
        }
        Some(StagedOp::UpdatePlaylistMeta {
            playlist_id: self.playlist_id.clone(),
            playlist_title: self.original_title.clone(),
            title,
            description,
            privacy,
        })
    }
}

/// Modal dialog for renaming a playlist, editing its description, or changing privacy.
pub fn show_playlist_meta_modal(
    ctx: &egui::Context,
    modal_state: &mut Option<PlaylistMetaModal>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) {
    let Some(form) = modal_state else {
        return;
    };
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *modal_state = None;
        return;
    }

    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("meta_modal_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(120));
        });

    let mut close = false;
    let mut save = false;
    egui::Window::new(format!("Edit Playlist: {}", form.original_title))
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Title:");
            ui.add(egui::TextEdit::singleline(&mut form.title).desired_width(320.0));
            ui.add_space(6.0);
            ui.label("Description:");
            ui.add(
                egui::TextEdit::multiline(&mut form.description)
                    .desired_width(320.0)
                    .desired_rows(3),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Privacy:");
                egui::ComboBox::from_id_salt("meta_privacy_combo")
                    .selected_text(format!("{:?}", form.privacy))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.privacy, PlaylistPrivacy::Private, "Private");
                        ui.selectable_value(
                            &mut form.privacy,
                            PlaylistPrivacy::Unlisted,
                            "Unlisted",
                        );
                        ui.selectable_value(&mut form.privacy, PlaylistPrivacy::Public, "Public");
                    });
            });
            ui.add_space(10.0);
            let valid_title = !form.title.trim().is_empty()
                && !form.title.contains('<')
                && !form.title.contains('>');
            if !valid_title {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 100, 100),
                    "Title must be non-empty and cannot contain '<' or '>'.",
                );
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(valid_title, egui::Button::new("Stage Metadata Edit"))
                    .clicked()
                {
                    save = true;
                }
                if ui.button("Cancel (Esc)").clicked() {
                    close = true;
                }
            });
        });

    if save {
        if let Some(op) = form.to_staged_op() {
            let label = form.original_title.clone();
            let eid = sync_queue.push(op);
            *toast = Some(ToastState::staged(
                format!("Staged metadata update for '{label}' in Sync."),
                Some(eid),
            ));
        } else {
            *toast = Some(ToastState::info(true, "No metadata changes to stage."));
        }
        *modal_state = None;
    } else if close {
        *modal_state = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renaming_playlist_emits_sparse_update_with_none_for_unchanged_fields() {
        let mut modal = PlaylistMetaModal::new(
            "PL1".to_string(),
            "Original".to_string(),
            "Keep this description".to_string(),
            PlaylistPrivacy::Unlisted,
        );
        assert!(modal.to_staged_op().is_none());

        modal.title = "  Renamed  ".to_string();
        let op = modal.to_staged_op().expect("sparse op");
        assert_eq!(
            op,
            StagedOp::UpdatePlaylistMeta {
                playlist_id: "PL1".to_string(),
                playlist_title: "Original".to_string(),
                title: Some("Renamed".to_string()),
                description: None,
                privacy: None,
            }
        );
    }
}
