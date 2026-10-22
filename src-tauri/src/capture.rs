//! Admission and lifetime of native capture requests, shared by all entrances.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Default)]
pub struct CaptureGate {
    active: Arc<AtomicBool>,
    append: Arc<Mutex<Option<AppendTarget>>>,
}

#[derive(Clone, Debug)]
pub struct AppendTarget {
    pub task_id: String,
    pub expected_revision: u64,
}

pub struct CapturePermit(Arc<AtomicBool>);

pub enum ShortcutPress {
    Ignored,
    Capture(Option<i32>),
}

impl Drop for CapturePermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl CaptureGate {
    pub fn append_target(&self) -> Option<AppendTarget> {
        self.append.lock().unwrap().clone()
    }
    pub fn set_append_target(&self, target: Option<AppendTarget>) {
        *self.append.lock().unwrap() = target;
    }

    pub fn shortcut_press(&self, target: Option<i32>) -> ShortcutPress {
        if self.active.load(Ordering::Acquire) {
            ShortcutPress::Ignored
        } else {
            ShortcutPress::Capture(target)
        }
    }

    pub fn start(
        &self,
        recording: bool,
        screenshot: bool,
        target: Option<i32>,
        begin: impl FnOnce(CapturePermit),
    ) {
        if recording || (!screenshot && target == Some(std::process::id() as i32)) {
            return;
        }
        // Reject before callers clear the view or start an OS capture. Hold the
        // permit through the complete native stream, including OCR and image data.
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        begin(CapturePermit(self.active.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct View {
        selected: Option<&'static str>,
        notice: Option<&'static str>,
        launches: usize,
    }

    #[test]
    fn repeated_selection_after_result_focus_preserves_the_result() {
        let gate = CaptureGate::default();
        let mut view = View::default();
        let own_pid = std::process::id() as i32;
        gate.start(false, false, Some(own_pid + 1), |_permit| {
            view.launches += 1;
            view.selected = Some("synthetic-first-result");
        });
        // The first capture focuses Yurelay. A second shortcut therefore has
        // Yurelay's PID, and must not reach clear_capture_view/native acquisition.
        gate.start(false, false, Some(own_pid), |_permit| {
            view.selected = None;
            view.launches += 1;
            view.notice = Some("请在其他应用中选中文字，再按划词快捷键。");
        });
        assert_eq!(view.selected, Some("synthetic-first-result"));
        assert!(view.notice.is_none());
        assert_eq!(view.launches, 1);
    }

    #[test]
    fn repeated_screenshot_does_not_clear_the_pending_capture_or_show_busy_error() {
        let gate = CaptureGate::default();
        let mut active = None;
        let mut view = View::default();
        gate.start(false, true, None, |permit| {
            active = Some(permit);
            view.launches += 1;
            view.selected = Some("synthetic-screenshot");
        });
        gate.start(false, true, None, |_permit| {
            view.selected = None;
            view.launches += 1;
            view.notice = Some("正在截图，请完成或取消当前选区。");
        });
        assert_eq!(view.selected, Some("synthetic-screenshot"));
        assert!(view.notice.is_none());
        assert_eq!(view.launches, 1);
        drop(active);
        gate.start(false, true, Some(std::process::id() as i32), |_permit| {
            view.launches += 1;
        });
        assert_eq!(
            view.launches, 2,
            "a finished capture must permit another screenshot, even from Yurelay"
        );
    }

    #[test]
    fn active_selection_blocks_both_shortcuts_but_release_allows_a_new_source() {
        let gate = CaptureGate::default();
        let target = Some(std::process::id() as i32 + 1);
        let mut active = None;
        gate.start(false, false, target, |permit| active = Some(permit));
        for screenshot in [false, true] {
            gate.start(false, screenshot, target, |_| {
                panic!("duplicate reached native capture")
            });
        }
        drop(active);
        let mut launched = false;
        gate.start(false, false, target, |_| launched = true);
        assert!(launched);
    }

    #[test]
    fn recording_and_early_errors_do_not_leave_capture_locked() {
        let gate = CaptureGate::default();
        gate.start(true, true, None, |_| panic!("recording triggered capture"));
        for _ in 0..2 {
            let mut called = false;
            gate.start(false, true, None, |_permit| called = true);
            assert!(called, "an early exit must release capture admission");
        }
    }

    #[test]
    fn second_key_down_while_busy_stays_ignored_after_first_capture_finishes() {
        for screenshot in [false, true] {
            let gate = CaptureGate::default();
            let mut active = None;
            let target = Some(std::process::id() as i32 + 1);
            gate.start(false, screenshot, target, |permit| active = Some(permit));
            let mut pressed = std::collections::HashMap::new();
            pressed
                .entry(7)
                .or_insert_with(|| gate.shortcut_press(target));
            drop(active);
            // OS auto-repeat must retain the first key-down's ignored state.
            pressed
                .entry(7)
                .or_insert_with(|| gate.shortcut_press(target));
            let mut selected = Some("synthetic-first-result");
            if let Some(ShortcutPress::Capture(target)) = pressed.remove(&7) {
                gate.start(false, screenshot, target, |_| selected = None);
            }
            assert_eq!(selected, Some("synthetic-first-result"));
        }
    }

    #[tokio::test]
    async fn permit_survives_native_events_until_the_stream_closes() {
        let gate = CaptureGate::default();
        let (events, mut native) = tokio::sync::mpsc::unbounded_channel();
        let mut task = None;
        gate.start(false, true, None, |permit| {
            task = Some(tokio::spawn(async move {
                let _capture = permit;
                while native.recv().await.is_some() {}
            }));
        });
        for event in ["captured", "text", "image"] {
            events.send(event).unwrap();
            tokio::task::yield_now().await;
            gate.start(false, true, None, |_| {
                panic!("capture restarted before native end")
            });
        }
        drop(events);
        task.unwrap().await.unwrap();
        let mut restarted = false;
        gate.start(false, true, None, |_| restarted = true);
        assert!(restarted);
    }

    #[test]
    fn concurrent_entrances_admit_only_one_capture() {
        let gate = CaptureGate::default();
        let mut permits = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|_| {
                    let gate = gate.clone();
                    scope.spawn(move || {
                        let mut permit = None;
                        gate.start(false, true, None, |accepted| permit = Some(accepted));
                        permit
                    })
                })
                .collect();
            for handle in handles {
                if let Some(permit) = handle.join().unwrap() {
                    permits.push(permit);
                }
            }
        });
        assert_eq!(permits.len(), 1);
    }
}
