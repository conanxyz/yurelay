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
    pub fn fit(&mut self) {
        self.manual = false;
        self.generation += 1;
    }
    pub fn accepts(&self, generation: Option<u64>) -> bool {
        !self.expanded
            && !self.manual
            && self.transition.is_none()
            && generation.is_none_or(|g| g == self.generation)
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

#[cfg(test)]
mod tests {
    #[test]
    fn stale_measurements_and_maximized_sizes_never_override_restore() {
        let mut s = super::WindowSizing::default();
        let old = s.generation;
        s.begin(true);
        assert!(!s.accepts(Some(old)));
        s.observe(true, false);
        s.begin(false);
        assert!(!s.accepts(None));
        s.observe(false, false);
        assert!(!s.accepts(Some(old)));
        assert!(s.accepts(Some(s.generation)));
    }
    #[test]
    fn user_size_survives_expand_restore_until_explicit_fit() {
        let mut s = super::WindowSizing::default();
        s.observe(false, true);
        s.begin(true);
        s.observe(true, false);
        s.begin(false);
        s.observe(false, false);
        assert!(!s.accepts(Some(s.generation)));
        s.fit();
        assert!(s.accepts(Some(s.generation)));
    }
}
