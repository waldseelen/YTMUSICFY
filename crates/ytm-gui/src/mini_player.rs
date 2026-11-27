use eframe::egui;

use crate::contracts::{MiniPlayerGeometry, MiniPlayerMode};

pub const MINI_DEFAULT_WIDTH: f32 = 360.0;
pub const MINI_COMPACT_HEIGHT: f32 = 136.0;
pub const MINI_EXPANDED_HEIGHT: f32 = 318.0;
pub const MINI_MIN_WIDTH: f32 = 300.0;
pub const MINI_MIN_HEIGHT: f32 = 124.0;
pub const MINI_SNAP_THRESHOLD: f32 = 18.0;
pub const MINI_EDGE_MARGIN: f32 = 12.0;

/// Runtime state for the borderless Mini Player secondary viewport.
#[derive(Debug, Clone, Default)]
pub struct MiniPlayerState {
    pub open: bool,
    pub mode: MiniPlayerMode,
    pub last_monitor_key: Option<String>,
    pub applied_geometry_for_key: Option<String>,
    pub was_pointer_down: bool,
}

impl MiniPlayerState {
    #[must_use]
    pub fn desired_size(&self, saved: Option<&MiniPlayerGeometry>) -> egui::Vec2 {
        let w = saved
            .map(|g| g.width.max(MINI_MIN_WIDTH))
            .unwrap_or(MINI_DEFAULT_WIDTH);
        let h = match self.mode {
            MiniPlayerMode::Compact => saved
                .filter(|g| g.mode == MiniPlayerMode::Compact)
                .map(|g| g.height.clamp(MINI_MIN_HEIGHT, 200.0))
                .unwrap_or(MINI_COMPACT_HEIGHT),
            MiniPlayerMode::Expanded => saved
                .filter(|g| g.mode == MiniPlayerMode::Expanded)
                .map(|g| g.height.clamp(240.0, 520.0))
                .unwrap_or(MINI_EXPANDED_HEIGHT),
        };
        egui::vec2(w, h)
    }
}

/// Stable monitor bucket key derived from `ViewportInfo::monitor_size`.
#[must_use]
pub fn monitor_key(monitor_size: Option<egui::Vec2>) -> String {
    match monitor_size {
        Some(sz) if sz.x > 0.0 && sz.y > 0.0 => {
            format!("{}x{}", sz.x.round() as u32, sz.y.round() as u32)
        }
        _ => "default".to_string(),
    }
}

/// Clamp a Mini Player top-left position so the full window stays inside the visible monitor.
#[must_use]
pub fn clamp_pos_to_monitor(
    pos: egui::Pos2,
    size: egui::Vec2,
    monitor_size: egui::Vec2,
) -> egui::Pos2 {
    if monitor_size.x <= 0.0 || monitor_size.y <= 0.0 {
        return pos;
    }
    let max_x = (monitor_size.x - size.x).max(0.0);
    let max_y = (monitor_size.y - size.y).max(0.0);
    egui::pos2(pos.x.clamp(0.0, max_x), pos.y.clamp(0.0, max_y))
}

/// Snap Mini Player edges to monitor borders when dragged within `threshold` logical pixels.
#[must_use]
pub fn snap_to_monitor_edges(
    pos: egui::Pos2,
    size: egui::Vec2,
    monitor_size: egui::Vec2,
    threshold: f32,
    margin: f32,
) -> egui::Pos2 {
    let clamped = clamp_pos_to_monitor(pos, size, monitor_size);
    if monitor_size.x <= 0.0 || monitor_size.y <= 0.0 {
        return clamped;
    }
    let right_edge = (monitor_size.x - size.x - margin).max(0.0);
    let bottom_edge = (monitor_size.y - size.y - margin).max(0.0);

    let x = if (clamped.x - margin).abs() <= threshold || clamped.x <= threshold {
        margin.min((monitor_size.x - size.x).max(0.0))
    } else if (clamped.x - right_edge).abs() <= threshold
        || (monitor_size.x - (clamped.x + size.x)).abs() <= threshold
    {
        right_edge
    } else {
        clamped.x
    };

    let y = if (clamped.y - margin).abs() <= threshold || clamped.y <= threshold {
        margin.min((monitor_size.y - size.y).max(0.0))
    } else if (clamped.y - bottom_edge).abs() <= threshold
        || (monitor_size.y - (clamped.y + size.y)).abs() <= threshold
    {
        bottom_edge
    } else {
        clamped.y
    };

    clamp_pos_to_monitor(egui::pos2(x, y), size, monitor_size)
}

/// Default bottom-right anchor position on a monitor when no saved layout exists.
#[must_use]
pub fn default_anchor_pos(size: egui::Vec2, monitor_size: egui::Vec2) -> egui::Pos2 {
    let raw = egui::pos2(
        monitor_size.x - size.x - MINI_EDGE_MARGIN * 2.0,
        monitor_size.y - size.y - MINI_EDGE_MARGIN * 3.0,
    );
    clamp_pos_to_monitor(raw, size, monitor_size)
}

/// Compute surface opacity multiplier for Spotify-style idle dimming.
#[must_use]
pub const fn effective_idle_alpha(opacity_on_idle: bool, hovered: bool, focused: bool) -> f32 {
    if !opacity_on_idle || hovered || focused {
        1.0
    } else {
        0.65
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_pos_keeps_mini_player_inside_visible_monitor() {
        let mon = egui::vec2(1920.0, 1080.0);
        let size = egui::vec2(360.0, 136.0);
        assert_eq!(
            clamp_pos_to_monitor(egui::pos2(-50.0, -20.0), size, mon),
            egui::pos2(0.0, 0.0)
        );
        assert_eq!(
            clamp_pos_to_monitor(egui::pos2(1900.0, 1050.0), size, mon),
            egui::pos2(1560.0, 944.0)
        );
    }

    #[test]
    fn snap_to_monitor_edges_snaps_near_all_four_borders() {
        let mon = egui::vec2(1920.0, 1080.0);
        let size = egui::vec2(360.0, 136.0);

        let top_left = snap_to_monitor_edges(egui::pos2(5.0, 4.0), size, mon, 18.0, 12.0);
        assert_eq!(top_left, egui::pos2(12.0, 12.0));

        let bottom_right = snap_to_monitor_edges(egui::pos2(1555.0, 938.0), size, mon, 18.0, 12.0);
        assert_eq!(bottom_right, egui::pos2(1548.0, 932.0));

        let mid = snap_to_monitor_edges(egui::pos2(600.0, 400.0), size, mon, 18.0, 12.0);
        assert_eq!(mid, egui::pos2(600.0, 400.0));
    }

    #[test]
    fn idle_alpha_dims_only_when_enabled_unhovered_and_unfocused() {
        assert!((effective_idle_alpha(false, false, false) - 1.0).abs() < f32::EPSILON);
        assert!((effective_idle_alpha(true, true, false) - 1.0).abs() < f32::EPSILON);
        assert!((effective_idle_alpha(true, false, true) - 1.0).abs() < f32::EPSILON);
        assert!(effective_idle_alpha(true, false, false) < 0.75);
    }
}
