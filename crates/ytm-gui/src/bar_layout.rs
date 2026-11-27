/// Responsive layout and scroll-input helpers shared by the Top Bar and Player Bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopBarLayout {
    pub show_brand_title: bool,
    pub center_search: bool,
    pub show_ribbon: bool,
    pub search_width: f32,
    pub side_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerBarLayout {
    pub compact_two_row: bool,
    pub side_width: f32,
    pub center_width: f32,
}

#[must_use]
pub fn effective_width(available_width: f32, font_scale: f32) -> f32 {
    available_width.max(0.0) / font_scale.clamp(0.8, 2.0)
}

impl TopBarLayout {
    #[must_use]
    pub fn from_width(available_width: f32, font_scale: f32) -> Self {
        let eff = effective_width(available_width, font_scale);
        let search_width = if eff >= 960.0 {
            (available_width * 0.28).clamp(200.0, 320.0)
        } else if eff >= 760.0 {
            (available_width * 0.24).clamp(150.0, 240.0)
        } else {
            (available_width * 0.28).clamp(120.0, 200.0)
        };
        let center_search = eff >= 760.0;
        let center_slot = search_width + 26.0;
        let side_width = if center_search {
            ((available_width - center_slot - 16.0) * 0.5).max(160.0)
        } else {
            (available_width * 0.34).max(110.0)
        };
        Self {
            show_brand_title: eff >= 720.0,
            center_search,
            show_ribbon: eff >= 820.0,
            search_width,
            side_width,
        }
    }
}

impl PlayerBarLayout {
    #[must_use]
    pub fn from_width(available_width: f32, font_scale: f32) -> Self {
        let eff = effective_width(available_width, font_scale);
        if eff < 780.0 {
            return Self {
                compact_two_row: true,
                side_width: (available_width * 0.48).max(120.0),
                center_width: available_width.max(240.0),
            };
        }
        // Equal left/right side columns keep the center transport locked to window center.
        let center_width = (available_width * 0.40).clamp(280.0, 440.0);
        let side_width = ((available_width - center_width - 16.0) * 0.5).max(160.0);
        Self {
            compact_two_row: false,
            side_width,
            center_width,
        }
    }
}

fn dominant_scroll_delta(scroll_y: f32, scroll_x: f32) -> f32 {
    if scroll_y.abs() >= scroll_x.abs() {
        scroll_y
    } else {
        scroll_x
    }
}

/// Compute new seek position (seconds) from pointer scroll delta (`±5s` per notch, `±1s` with Shift).
#[must_use]
pub fn scroll_seek_target(
    current_sec: f64,
    duration_sec: f64,
    scroll_y: f32,
    scroll_x: f32,
    shift: bool,
) -> Option<f64> {
    if duration_sec <= 0.0 {
        return None;
    }
    let delta = dominant_scroll_delta(scroll_y, scroll_x);
    if delta.abs() < 0.05 {
        return None;
    }
    let step_sec = if shift { 1.0 } else { 5.0 };
    let change = (f64::from(delta) / 40.0) * step_sec;
    Some((current_sec + change).clamp(0.0, duration_sec))
}

/// Compute new volume (`0..=100`) from pointer scroll delta (`±5%` per notch, `±1%` with Shift).
#[must_use]
pub fn scroll_volume_target(
    current_vol: f64,
    scroll_y: f32,
    scroll_x: f32,
    shift: bool,
) -> Option<f64> {
    let delta = dominant_scroll_delta(scroll_y, scroll_x);
    if delta.abs() < 0.05 {
        return None;
    }
    let step_pct = if shift { 1.0 } else { 5.0 };
    let change = (f64::from(delta) / 40.0) * step_pct;
    Some((current_vol + change).clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_bar_layout_collapses_progressively_without_overlap() {
        let wide = TopBarLayout::from_width(1200.0, 1.0);
        assert!(wide.show_brand_title && wide.center_search && wide.show_ribbon);
        assert!(wide.side_width * 2.0 + wide.search_width <= 1200.0);

        let med = TopBarLayout::from_width(780.0, 1.0);
        assert!(med.show_brand_title && med.center_search && !med.show_ribbon);

        let narrow = TopBarLayout::from_width(600.0, 1.0);
        assert!(!narrow.show_brand_title && !narrow.center_search && !narrow.show_ribbon);
    }

    #[test]
    fn player_bar_switches_to_two_row_when_narrow_or_scaled() {
        let wide = PlayerBarLayout::from_width(1100.0, 1.0);
        assert!(!wide.compact_two_row);
        assert!((wide.side_width * 2.0 + wide.center_width - 1100.0).abs() <= 20.0);

        let narrow = PlayerBarLayout::from_width(700.0, 1.0);
        assert!(narrow.compact_two_row);

        // 1000px at 1.4x font scale has effective width ~714px -> switches to 2-row compact mode
        let scaled = PlayerBarLayout::from_width(1000.0, 1.4);
        assert!(scaled.compact_two_row);
    }

    #[test]
    fn scroll_seek_and_volume_respect_steps_and_bounds() {
        let s = scroll_seek_target(30.0, 200.0, 40.0, 0.0, false).unwrap();
        assert!((s - 35.0).abs() < 1e-6);
        let fine = scroll_seek_target(30.0, 200.0, -40.0, 0.0, true).unwrap();
        assert!((fine - 29.0).abs() < 1e-6);
        assert_eq!(scroll_seek_target(0.0, 0.0, 40.0, 0.0, false), None);

        let v = scroll_volume_target(98.0, 40.0, 0.0, false).unwrap();
        assert!((v - 100.0).abs() < 1e-6);
        let v_fine = scroll_volume_target(50.0, 0.0, -40.0, true).unwrap();
        assert!((v_fine - 49.0).abs() < 1e-6);
    }
}
