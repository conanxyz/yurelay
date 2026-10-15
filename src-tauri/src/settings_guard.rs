use serde::Serialize;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Intent {
    Close,
    Quit,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Request {
    pub id: u64,
    pub intent: Intent,
}

#[derive(Default)]
struct State {
    ready: bool,
    next_id: u64,
    pending: Option<Request>,
    quit_confirmed: bool,
}

#[derive(Default)]
pub struct SettingsGuard(Mutex<State>);

impl SettingsGuard {
    pub fn enable(&self) {
        self.0.lock().unwrap().ready = true;
    }

    pub fn request(&self, intent: Intent) -> Option<Request> {
        let mut state = self.0.lock().unwrap();
        if !state.ready || (intent == Intent::Quit && state.quit_confirmed) {
            return None;
        }
        if let Some(pending) = state.pending
            && (pending.intent == Intent::Quit || intent == Intent::Close)
        {
            return Some(pending);
        }
        state.next_id += 1;
        let request = Request {
            id: state.next_id,
            intent,
        };
        state.pending = Some(request);
        Some(request)
    }

    pub fn confirm(&self, id: u64) -> Result<Intent, String> {
        let mut state = self.0.lock().unwrap();
        let pending = state
            .pending
            .filter(|pending| pending.id == id)
            .ok_or("关闭请求已改变，请重新确认。")?;
        state.pending = None;
        state.quit_confirmed = pending.intent == Intent::Quit;
        Ok(pending.intent)
    }

    pub fn cancel(&self, id: u64) {
        let mut state = self.0.lock().unwrap();
        if state.pending.is_some_and(|pending| pending.id == id) {
            state.pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_cannot_authorize_a_later_quit_and_cancel_keeps_guard_enabled() {
        let guard = SettingsGuard::default();
        guard.enable();
        let close = guard.request(Intent::Close).unwrap();
        let quit = guard.request(Intent::Quit).unwrap();
        assert!(guard.confirm(close.id).is_err());
        guard.cancel(quit.id);
        assert!(guard.confirm(quit.id).is_err());
        let retry = guard.request(Intent::Quit).unwrap();
        assert_eq!(guard.confirm(retry.id).unwrap(), Intent::Quit);
        assert!(guard.request(Intent::Quit).is_none());
    }
}
