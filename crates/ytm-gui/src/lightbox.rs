use eframe::egui;

use crate::artwork::ArtworkCache;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button};
use crate::theme::Palette;

/// Explicit state machine for the Album-Art Lightbox modal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LightboxState {
    #[default]
    Closed,
    Open {
        title: String,
        subtitle: String,
        image_url: String,
    },
}

impl LightboxState {
    pub fn open(
        &mut self,
        title: impl Into<String>,
        subtitle: impl Into<String>,
        image_url: impl Into<String>,
    ) {
        *self = Self::Open {
            title: title.into(),
            subtitle: subtitle.into(),
            image_url: image_url.into(),
        };
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }
}

/// Render centered album-art Lightbox modal; closes on outside click or `Esc`.
pub fn show_lightbox(
    ctx: &egui::Context,
    state: &mut LightboxState,
    artwork: &mut ArtworkCache,
    palette: &Palette,
    lang: ResolvedLang,
) {
    let LightboxState::Open {
        title,
        subtitle,
        image_url,
    } = state.clone()
    else {
        return;
    };

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.close();
        return;
    }

    let screen = ctx.screen_rect();
    let mut clicked_outside = false;
    egui::Area::new(egui::Id::new("dalga_lightbox_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let resp = ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(175));
            if resp.clicked() {
                clicked_outside = true;
            }
        });

    let mut close_btn = false;
    let card_resp = egui::Area::new(egui::Id::new("dalga_lightbox_card"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .fill(palette.bg_panel)
                .stroke(egui::Stroke::new(1.0_f32, palette.border_subtle))
                .corner_radius(10.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_width(360.0);
                    ui.horizontal(|ui| {
                        ui.strong(&title);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(
                                ui,
                                Icon::Close,
                                14.0,
                                true,
                                tr(lang, Key::RibbonDismiss),
                            )
                            .clicked()
                            {
                                close_btn = true;
                            }
                        });
                    });
                    if !subtitle.is_empty() {
                        ui.colored_label(palette.text_muted, &subtitle);
                    }
                    ui.add_space(10.0);
                    let size = egui::vec2(328.0, 328.0);
                    if let Some(tex) = artwork.texture_for(ctx, &image_url) {
                        ui.add(egui::Image::new((tex.id(), size)).corner_radius(8.0));
                    } else {
                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        ui.painter().rect_filled(rect, 8.0, palette.bg_elevated);
                        Icon::Music.image(64.0, palette.text_muted).paint_at(
                            ui,
                            egui::Rect::from_center_size(rect.center(), egui::vec2(64.0, 64.0)),
                        );
                    }
                });
        })
        .response;

    if close_btn || clicked_outside {
        state.close();
        return;
    }
    if ctx.input(|i| i.pointer.any_pressed())
        && let Some(pos) = ctx.pointer_interact_pos()
        && !card_resp.rect.contains(pos)
    {
        state.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightbox_open_and_close_reducer_transitions() {
        let mut lb = LightboxState::default();
        assert!(!lb.is_open());

        lb.open("Discovery", "Daft Punk", "https://example.com/art.jpg");
        assert!(lb.is_open());
        assert_eq!(
            lb,
            LightboxState::Open {
                title: "Discovery".to_string(),
                subtitle: "Daft Punk".to_string(),
                image_url: "https://example.com/art.jpg".to_string(),
            }
        );

        lb.close();
        assert!(!lb.is_open());
    }
}
