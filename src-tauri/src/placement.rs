//! Physical screen coordinates; keep restored windows inside a visible work area.
pub fn clamp_position(position: [i32; 2], area: [i32; 4], size: [i32; 2], margin: i32) -> [i32; 2] {
    let [x, y, w, h] = area;
    [
        position[0].clamp(x + margin, (x + w - size[0] - margin).max(x + margin)),
        position[1].clamp(y + margin, (y + h - size[1] - margin).max(y + margin)),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positions_survive_negative_origin_and_display_size_changes() {
        assert_eq!(
            clamp_position([-3000, -300], [-1920, 0, 1920, 1080], [460, 560], 16),
            [-1904, 16]
        );
        assert_eq!(
            clamp_position([1200, 800], [0, 0, 1024, 768], [460, 560], 16),
            [548, 192]
        );
        assert_eq!(
            clamp_position([300, 120], [0, 0, 1920, 1080], [460, 560], 16),
            [300, 120]
        );
    }
}

/// Logical coordinates. Keep the reading column anchored when adding/removing
/// a left sidebar; only move/shrink it when the monitor work area requires it.
pub fn sidebar_bounds(
    position: [f64; 2],
    size: [f64; 2],
    area: [f64; 4],
    old: f64,
    requested: u32,
) -> ([f64; 2], [f64; 2], u32) {
    let max_width = (area[2] - 24.0).max(360.0);
    let sidebar = if requested == 0 {
        0.0
    } else {
        f64::from(requested).min((max_width - 360.0).max(0.0))
    };
    let width = ((size[0] - old).max(360.0) + sidebar).min(max_width);
    let height = size[1].min((area[3] - 24.0).max(220.0));
    let x = (position[0] + old - sidebar).clamp(
        area[0] + 12.0,
        (area[0] + area[2] - width - 12.0).max(area[0] + 12.0),
    );
    let y = position[1].clamp(
        area[1] + 12.0,
        (area[1] + area[3] - height - 12.0).max(area[1] + 12.0),
    );
    ([x, y], [width, height], sidebar.floor() as u32)
}

#[cfg(test)]
mod sidebar_tests {
    use super::*;
    #[test]
    fn opening_and_collapsing_preserve_reading_anchor_and_width() {
        let area = [0.0, 0.0, 1440.0, 900.0];
        let (p, s, w) = sidebar_bounds([400.0, 100.0], [460.0, 600.0], area, 0.0, 220);
        assert_eq!((p, s, w), ([180.0, 100.0], [680.0, 600.0], 220));
        let (p, s, w) = sidebar_bounds(p, s, area, 220.0, 40);
        assert_eq!((p, s, w), ([360.0, 100.0], [500.0, 600.0], 40));
        assert_eq!(
            sidebar_bounds(p, s, area, 40.0, 0),
            ([400.0, 100.0], [460.0, 600.0], 0)
        );
    }
    #[test]
    fn sidebar_stays_inside_negative_and_small_work_areas() {
        let (p, s, _) = sidebar_bounds(
            [-1900.0, 40.0],
            [460.0, 600.0],
            [-1920.0, 0.0, 1920.0, 1080.0],
            0.0,
            220,
        );
        assert_eq!(p[0], -1908.0);
        assert!(p[0] + s[0] <= -12.0);
        let (p, s, w) = sidebar_bounds(
            [100.0, 100.0],
            [460.0, 600.0],
            [0.0, 0.0, 600.0, 700.0],
            0.0,
            220,
        );
        assert_eq!(w, 216);
        assert_eq!(s[0], 576.0);
        assert_eq!(s[0] - f64::from(w), 360.0);
        assert!(p[0] + s[0] <= 588.0);
    }
}
