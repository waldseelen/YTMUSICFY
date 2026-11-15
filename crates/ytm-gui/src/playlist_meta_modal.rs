use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::PlaylistPrivacy;

use crate::worker_msgs::WorkerCmd;

/// Form state for editing an owned playlist's title, description, and privacy.
#[derive(Debug, Clone)]
pub struct PlaylistMetaModal {
    pub playlist_id: String,
    pub original_title: String,
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
            title,
            description,
            privacy,
        }
    }
}

/// Modal dialog for renaming a playlist, editing its description, or changing privacy.
pub fn show_playlist_meta_modal(
    ctx: &egui::Context,
    modal_state: &mut Option<PlaylistMetaModal>,
    cmd_tx: &Sender<WorkerCmd>,
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
                    .add_enabled(valid_title, egui::Button::new("Save Changes"))
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
        let _ = cmd_tx.send(WorkerCmd::UpdatePlaylistMeta {
            playlist_id: form.playlist_id.clone(),
            title: Some(form.title.trim().to_string()),
            description: Some(form.description.clone()),
            privacy: Some(form.privacy),
        });
        *modal_state = None;
    } else if close {
        *modal_state = None;
    }
}
