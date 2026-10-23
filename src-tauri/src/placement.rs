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
