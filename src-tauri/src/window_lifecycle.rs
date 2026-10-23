//! Window presentation intent, independent of the OS event queue.
use std::sync::{Arc, Mutex};

pub struct SystemDialogGuard(Arc<Mutex<WindowLifecycle>>);

impl SystemDialogGuard {
    pub fn new(window: Arc<Mutex<WindowLifecycle>>) -> Self {
        {
            let mut state = window.lock().unwrap();
            state.generation += 1;
            state.system_dialogs += 1;
        }
        Self(window)
    }
}

impl Drop for SystemDialogGuard {
    fn drop(&mut self) {
        let mut state = self.0.lock().unwrap();
        state.system_dialogs -= 1;
        state.generation += 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureEpoch(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlurTicket(u64);

pub struct WindowLifecycle {
    // Every new intent invalidates asynchronous work from a previous blur.
    generation: u64,
    // Only hiding cancels presentation of a capture already in flight.
    hidden_epoch: u64,
    visible: bool,
    focused: bool,
    pinned: bool,
    system_dialogs: usize,
}

impl Default for WindowLifecycle {
    fn default() -> Self {
        Self {
            generation: 0,
            hidden_epoch: 0,
            visible: true,
            focused: true,
            pinned: false,
            system_dialogs: 0,
        }
    }
}

impl WindowLifecycle {
    pub fn capture_started(&mut self) -> CaptureEpoch {
        self.generation += 1;
        CaptureEpoch(self.hidden_epoch)
    }

    pub fn show(&mut self) {
        self.generation += 1;
        self.visible = true;
    }

    pub fn focus_changed(&mut self, focused: bool) -> Option<BlurTicket> {
        self.generation += 1;
        self.focused = focused;
        (!focused && self.visible && !self.pinned && self.system_dialogs == 0)
            .then_some(BlurTicket(self.generation))
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        self.generation += 1;
        self.pinned = pinned;
    }

    pub fn pinned(&self) -> bool {
        self.pinned
    }

    pub fn hide(&mut self) {
        self.generation += 1;
        self.visible = false;
        self.hidden_epoch += 1;
    }

    pub fn auto_hide(&mut self, ticket: BlurTicket, external_focus: bool) -> bool {
        if !external_focus
            || self.pinned
            || ticket.0 != self.generation
            || self.focused
            || !self.visible
            || self.system_dialogs > 0
        {
            return false;
        }
        self.hide();
        true
    }

    pub fn capture_is_current(&self, epoch: CaptureEpoch) -> bool {
        self.hidden_epoch == epoch.0
    }

    pub fn present_capture(&mut self, epoch: CaptureEpoch) -> bool {
        if !self.capture_is_current(epoch) {
            return false;
        }
        self.show();
        true
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn reading(&self) -> bool {
        self.visible && self.focused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_dialog_preserves_the_window_and_invalidates_delayed_blur() {
        let window = Arc::new(Mutex::new(WindowLifecycle::default()));
        let old_blur = window.lock().unwrap().focus_changed(false).unwrap();
        let dialog = SystemDialogGuard::new(window.clone());
        assert!(window.lock().unwrap().focus_changed(false).is_none());
        assert!(!window.lock().unwrap().auto_hide(old_blur, true));
        let nested = SystemDialogGuard::new(window.clone());
        drop(dialog);
        assert!(window.lock().unwrap().focus_changed(false).is_none());
        drop(nested);
        assert!(!window.lock().unwrap().auto_hide(old_blur, true));
        assert!(window.lock().unwrap().visible());
        let blur = window.lock().unwrap().focus_changed(false).unwrap();
        assert!(window.lock().unwrap().auto_hide(blur, true));
    }

    #[test]
    fn finishing_system_authorization_does_not_reopen_an_explicitly_hidden_window() {
        let window = Arc::new(Mutex::new(WindowLifecycle::default()));
        let dialog = SystemDialogGuard::new(window.clone());
        window.lock().unwrap().hide();
        drop(dialog);
        assert!(!window.lock().unwrap().visible());
    }

    #[test]
    fn delayed_blur_cannot_cancel_a_new_capture_before_its_result() {
        let mut window = WindowLifecycle::default();
        let blur = window.focus_changed(false).unwrap();
        let capture = window.capture_started();
        assert!(!window.auto_hide(blur, true));
        assert!(window.present_capture(capture));
    }

    #[test]
    fn delayed_blur_cannot_hide_a_new_result_before_focused_true_arrives() {
        let mut window = WindowLifecycle::default();
        let blur = window.focus_changed(false).unwrap();
        let capture = window.capture_started();
        assert!(window.present_capture(capture));
        assert!(!window.auto_hide(blur, true));
        assert!(window.visible());
    }

    #[test]
    fn presenting_an_existing_capture_invalidates_an_older_blur() {
        let mut window = WindowLifecycle::default();
        let capture = window.capture_started();
        let blur = window.focus_changed(false).unwrap();
        assert!(window.present_capture(capture));
        assert!(!window.auto_hide(blur, true));
    }

    #[test]
    fn opening_the_window_invalidates_an_older_blur() {
        let mut window = WindowLifecycle::default();
        let blur = window.focus_changed(false).unwrap();
        window.show();
        assert!(!window.auto_hide(blur, true));
        assert!(window.visible());
    }

    #[test]
    fn a_later_focus_cycle_replaces_the_old_blur() {
        let mut window = WindowLifecycle::default();
        let old_blur = window.focus_changed(false).unwrap();
        window.focus_changed(true);
        let new_blur = window.focus_changed(false).unwrap();
        assert!(!window.auto_hide(old_blur, true));
        assert!(window.auto_hide(new_blur, true));
        assert!(!window.visible());
    }

    #[test]
    fn current_blur_hides_the_window_once() {
        let mut window = WindowLifecycle::default();
        let blur = window.focus_changed(false).unwrap();
        assert!(window.auto_hide(blur, true));
        let after_hide = window.hidden_epoch;
        assert!(!window.auto_hide(blur, true));
        assert_eq!(window.hidden_epoch, after_hide);
        assert!(window.focus_changed(false).is_none());
    }

    #[test]
    fn explicit_hide_cancels_the_capture_and_invalidates_pending_blur() {
        let mut window = WindowLifecycle::default();
        let capture = window.capture_started();
        let blur = window.focus_changed(false).unwrap();
        window.hide();
        let after_hide = window.hidden_epoch;
        assert!(!window.present_capture(capture));
        assert!(!window.auto_hide(blur, true));
        assert_eq!(window.hidden_epoch, after_hide);
        assert!(!window.visible());
    }

    #[test]
    fn explicit_hide_cancels_presentation_already_queued_for_the_main_thread() {
        let mut window = WindowLifecycle::default();
        let capture = window.capture_started();
        assert!(window.present_capture(capture));
        window.hide();
        // This is the final check inside the queued native presentation closure.
        assert!(!window.capture_is_current(capture));
        assert!(!window.visible());
    }

    #[test]
    fn a_new_capture_can_reopen_after_explicit_hide() {
        let mut window = WindowLifecycle::default();
        let previous = window.capture_started();
        window.hide();
        let next = window.capture_started();
        assert!(!window.present_capture(previous));
        assert!(window.present_capture(next));
        assert!(window.visible());
    }

    #[test]
    fn internal_overlay_blur_does_not_cancel_the_capture() {
        let mut window = WindowLifecycle::default();
        let capture = window.capture_started();
        let blur = window.focus_changed(false).unwrap();
        assert!(!window.auto_hide(blur, false));
        assert!(window.present_capture(capture));
    }
    #[test]
    fn pinning_cancels_delayed_blur_and_keeps_the_window_visible() {
        let mut window = WindowLifecycle::default();
        let old_blur = window.focus_changed(false).unwrap();
        window.set_pinned(true);
        assert!(window.pinned());
        assert!(!window.auto_hide(old_blur, true));
        assert!(window.focus_changed(false).is_none());
        assert!(window.visible());
        window.set_pinned(false);
        assert!(!window.auto_hide(old_blur, true));
        window.focus_changed(true);
        let fresh_blur = window.focus_changed(false).unwrap();
        assert!(window.auto_hide(fresh_blur, true));
    }

    #[test]
    fn explicit_hide_still_works_while_pinned_and_pin_survives_reopening() {
        let mut window = WindowLifecycle::default();
        window.set_pinned(true);
        let capture = window.capture_started();
        window.hide();
        assert!(!window.visible());
        assert!(!window.present_capture(capture));
        assert!(window.pinned());
        window.show();
        assert!(window.visible());
        assert!(window.focus_changed(false).is_none());
    }
}
