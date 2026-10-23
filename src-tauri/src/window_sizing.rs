//! Native window mode owns sizing. Content measurements cannot override a drag
//! or a newer maximize/fullscreen transition.
use serde::Serialize;
#[derive(Clone, Copy, Default, Serialize)]
pub struct WindowSizing {
    pub expanded: bool,
    pub manual: bool,
    pub generation: u64,
    #[serde(skip)]
    transition: Option<bool>,
    #[serde(skip)]
    request: u64,
}
impl WindowSizing {
    pub fn begin(&mut self, target: bool) -> u64 {
        self.generation += 1;
        self.request += 1;
        self.transition = Some(target);
        self.request
    }
    fn pending(&self, request: u64, target: bool) -> bool {
        self.request == request && self.transition == Some(target)
    }
    pub fn observe(&mut self, expanded: bool, live_resize: bool) -> bool {
        let changed = self.expanded != expanded
            || (!expanded && live_resize && !self.manual)
            || self.transition == Some(expanded);
        if changed {
            self.generation += 1;
        }
        self.expanded = expanded;
        if self.transition == Some(expanded) {
            self.transition = None;
        }
        if !expanded && live_resize && self.transition.is_none() {
            self.manual = true;
        }
        changed
    }
    pub fn settled(&self) -> bool {
        !self.expanded && self.transition.is_none()
    }
}
/// Some AppKit transitions (notably hidden fullscreen windows) do not resize.
/// Confirm the native mode even in that case, without relying on a resize event.
pub fn set_expanded(app: &tauri::AppHandle, expanded: bool) -> Result<(), String> {
    use crate::state::{self, AppState};
    use tauri::{Emitter, Manager};
    let window = app.get_webview_window("main").ok_or("窗口不可用。")?;
    let request = app
        .state::<AppState>()
        .sizing
        .lock()
        .unwrap()
        .begin(expanded);
    // AppKit keeps the actual normal bounds; do not impose a fixed compact size.
    let result = if expanded {
        window.maximize().map_err(|_| "无法展开窗口。")
    } else if window.is_fullscreen().unwrap_or(false) {
        window.set_fullscreen(false).map_err(|_| "无法退出全屏。")
    } else {
        window.unmaximize().map_err(|_| "无法恢复小窗口。")
    };
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        for attempt in 0..40 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let state = handle.state::<AppState>();
            if !state.sizing.lock().unwrap().pending(request, expanded) {
                return;
            }
            let actual =
                window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false);
            let mut sizing = state.sizing.lock().unwrap();
            if !sizing.pending(request, expanded) {
                return;
            }
            if actual == expanded || attempt == 39 {
                sizing.transition = None;
                sizing.expanded = actual;
                sizing.generation += 1;
                let snapshot = *sizing;
                drop(sizing);
                let _ = window.emit("window-mode-changed", snapshot);
                if actual != expanded {
                    state::notice(&handle, "窗口调整未完成，请重试。".into());
                }
                return;
            }
        }
    });
    result.map_err(String::from)
}

pub fn initial_size(path: &std::path::Path, area: [f64; 2]) -> [f64; 2] {
    let saved = std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice::<[f64; 2]>(&data).ok())
        .filter(|s| s.iter().all(|n| n.is_finite() && *n > 0.0));
    let [w, h] = saved.unwrap_or([520.0, 560.0]);
    [
        w.clamp(360.0, (area[0] - 32.0).max(360.0)),
        h.clamp(320.0, (area[1] - 32.0).max(320.0)),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn manual_size_mode_survives_native_zoom() {
        let mut s = super::WindowSizing::default();
        assert!(s.settled());
        s.observe(false, true);
        s.begin(true);
        assert!(!s.settled());
        s.observe(true, false);
        s.begin(false);
        assert!(!s.settled());
        s.observe(false, false);
        assert!(s.settled() && s.manual);
    }
    #[test]
    fn saved_size_reopens_and_only_screen_bounds_constrain_it() {
        let path = std::env::temp_dir().join(format!("yurelay-size-{}.json", uuid::Uuid::new_v4()));
        assert_eq!(super::initial_size(&path, [1440.0, 900.0]), [520.0, 560.0]);
        std::fs::write(&path, b"[680,640]").unwrap();
        assert_eq!(super::initial_size(&path, [1440.0, 900.0]), [680.0, 640.0]);
        assert_eq!(super::initial_size(&path, [600.0, 600.0]), [568.0, 568.0]);
        std::fs::write(&path, b"corrupt").unwrap();
        assert_eq!(super::initial_size(&path, [1440.0, 900.0]), [520.0, 560.0]);
        std::fs::remove_file(path).unwrap();
    }
}
