//! Small platform boundary. No translation or storage logic belongs here.

use serde::Serialize;
use tokio::sync::mpsc;

mod language;
pub use language::{OcrBlock, OcrLayout, TokenRange, token_ranges};
mod pronunciation;
pub use pronunciation::{
    DictionaryDetails, SpeechVoice, lookup_dictionary_details, speak_word, speech_voices,
};

#[derive(Debug, Clone, Serialize)]
pub struct Permissions {
    pub accessibility: bool,
    pub screen: bool,
}

#[derive(Debug)]
pub enum NativeEvent {
    Captured,
    Image(Vec<u8>),
    Text(String),
    Layout(OcrLayout),
    Error(String),
    OcrError(String),
    ImageError(String),
    Cancelled,
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::{c_char, c_void};

    type Callback = unsafe extern "C" fn(*mut c_void, i32, *const u8, usize);

    unsafe extern "C" {
        fn ad_choose_export(name: *const c_char, context: *mut c_void, callback: Callback);
        fn ad_permissions() -> u32;
        fn ad_set_locale(english: bool);
        pub(super) fn ad_dictionary_lookup(
            input: *const c_char,
            output: *mut u8,
            capacity: usize,
        ) -> usize;
        pub(super) fn ad_token_ranges(
            input: *const c_char,
            output: *mut u8,
            capacity: usize,
        ) -> usize;
        pub(super) fn ad_lemma(input: *const c_char, output: *mut u8, capacity: usize) -> usize;
        pub(super) fn ad_open_dictionary(kind: i32, input: *const c_char);
        fn ad_detect_language(text: *const c_char, output: *mut u8, capacity: usize) -> usize;
        fn ad_request_permission(kind: i32);
        fn ad_select_text(context: *mut c_void, callback: Callback);
        fn ad_selection_target_pid() -> i32;
        fn ad_select_text_for_pid(pid: i32, context: *mut c_void, callback: Callback);
        fn ad_capture(context: *mut c_void, callback: Callback);
        fn ad_copy_text(text: *const c_char);
        fn ad_configure_window(window: *mut c_void);
        fn ad_present_window(window: *mut c_void);
        fn ad_dismiss_window(window: *mut c_void);
    }

    // SAFETY: native retains context until the final tag 5 callback and serializes
    // callbacks per session. The borrowed byte buffer is valid only during this call.
    // We copy its bytes before returning. No Rust unwind may cross the C boundary.
    unsafe extern "C" fn receive(context: *mut c_void, tag: i32, data: *const u8, len: usize) {
        let _ = std::panic::catch_unwind(|| {
            if context.is_null() {
                return;
            }
            if tag == 5 {
                // SAFETY: tag 5 is delivered exactly once, after all other callbacks.
                drop(unsafe {
                    Box::from_raw(context.cast::<mpsc::UnboundedSender<NativeEvent>>())
                });
                return;
            }
            // SAFETY: the box remains owned by native until tag 5.
            let tx = unsafe { &*context.cast::<mpsc::UnboundedSender<NativeEvent>>() };
            if len > 64 * 1024 * 1024 {
                let _ = tx.send(NativeEvent::Error("截图过大，请缩小选区。".into()));
                return;
            }
            let bytes = if len == 0 || data.is_null() {
                &[][..]
            } else {
                // SAFETY: native promises len initialized bytes for this invocation.
                unsafe { std::slice::from_raw_parts(data, len) }
            };
            let event = match tag {
                1 => NativeEvent::Image(bytes.to_vec()),
                2 => NativeEvent::Text(String::from_utf8_lossy(bytes).into_owned()),
                3 => NativeEvent::Error(String::from_utf8_lossy(bytes).into_owned()),
                4 => NativeEvent::Cancelled,
                6 => NativeEvent::OcrError(String::from_utf8_lossy(bytes).into_owned()),
                7 => NativeEvent::ImageError(String::from_utf8_lossy(bytes).into_owned()),
                8 => NativeEvent::Captured,
                9 => match serde_json::from_slice(bytes) {
                    Ok(layout) => NativeEvent::Layout(layout),
                    Err(_) => return,
                },
                _ => return,
            };
            let _ = tx.send(event);
        });
    }

    pub async fn choose_export(name: &str) -> Result<Option<String>, String> {
        let name = std::ffi::CString::new(name).map_err(|_| "文件名无效。")?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let context = Box::into_raw(Box::new(tx)).cast();
        // SAFETY: native copies name and ends the callback session exactly once.
        unsafe { ad_choose_export(name.as_ptr(), context, receive) };
        match rx.recv().await {
            Some(NativeEvent::Text(path)) => Ok(Some(path)),
            Some(NativeEvent::Cancelled) => Ok(None),
            _ => Err("无法打开导出对话框。".into()),
        }
    }
    pub fn set_locale(english: bool) {
        // SAFETY: native only updates an atomic flag.
        unsafe {
            ad_set_locale(english);
        }
    }

    pub fn detect_language(text: &str) -> Option<String> {
        let input = std::ffi::CString::new(text).ok()?;
        let mut output = [0u8; 32];
        // SAFETY: the native function reads input synchronously and writes at
        // most output.len() bytes. It retains neither pointer.
        let len = unsafe { ad_detect_language(input.as_ptr(), output.as_mut_ptr(), output.len()) };
        if len == 0 || len >= output.len() {
            return None;
        }
        String::from_utf8(output[..len].to_vec()).ok()
    }

    pub fn permissions() -> Permissions {
        // SAFETY: these are thread-safe system permission preflight checks.
        let flags = unsafe { ad_permissions() };
        Permissions {
            accessibility: flags & 1 != 0,
            screen: flags & 2 != 0,
        }
    }

    pub fn request_permission(kind: &str) -> Result<(), String> {
        let kind = match kind {
            "accessibility" => 1,
            "screen" => 2,
            _ => return Err("未知权限类型".into()),
        };
        // SAFETY: implementation dispatches AppKit work to the main queue.
        unsafe {
            ad_request_permission(kind);
        }
        Ok(())
    }

    pub fn start(screenshot: bool) -> mpsc::UnboundedReceiver<NativeEvent> {
        start_for_target(screenshot, None)
    }

    pub fn selection_target() -> Option<i32> {
        // SAFETY: native reads NSWorkspace on the main queue and returns only a PID.
        let pid = unsafe { ad_selection_target_pid() };
        (pid > 0).then_some(pid)
    }

    pub fn start_for_target(
        screenshot: bool,
        target: Option<i32>,
    ) -> mpsc::UnboundedReceiver<NativeEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let context = Box::into_raw(Box::new(tx)).cast();
        // SAFETY: context is exclusively owned by the callback contract until tag 5.
        // Each session emits at most one image, one text and a terminal error/cancel.
        unsafe {
            if screenshot {
                ad_capture(context, receive);
            } else if let Some(pid) = target {
                ad_select_text_for_pid(pid, context, receive);
            } else {
                ad_select_text(context, receive);
            }
        }
        rx
    }

    pub fn copy_text(text: &str) -> Result<(), String> {
        let c = std::ffi::CString::new(text).map_err(|_| "文字包含无效字符")?;
        // SAFETY: native copies the string synchronously, then dispatches pasteboard access.
        unsafe {
            ad_copy_text(c.as_ptr());
        }
        Ok(())
    }

    /// # Safety
    /// `window` must be a live NSWindow and caller must be on the main thread.
    pub unsafe fn present_window(window: *mut c_void) {
        unsafe {
            ad_present_window(window);
        }
    }

    /// # Safety
    /// `window` must be a live NSWindow and caller must be on the main thread.
    pub unsafe fn dismiss_window(window: *mut c_void) {
        unsafe {
            ad_dismiss_window(window);
        }
    }

    /// # Safety
    /// `window` must refer to a live NSWindow. Caller must run on the main thread.
    pub unsafe fn configure_window(window: *mut c_void) {
        unsafe {
            ad_configure_window(window);
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::*;

// Stable pre-rename Keychain service: keep existing keys through the Yurelay rename.
#[cfg(target_os = "macos")]
const CREDENTIAL_SERVICE: &str = "com.autodict.desktop.providers";

#[cfg(target_os = "macos")]
static CREDENTIAL_ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(target_os = "macos")]
fn with_credential_access<T>(
    allow_prompt: bool,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    use security_framework::os::macos::keychain::SecKeychain;
    // File-based keychains ignore per-query authentication UI options. Their
    // interaction switch is process-wide, so serialize reads AND writes.
    let _access = CREDENTIAL_ACCESS
        .lock()
        .map_err(|_| "读取系统凭据失败。".to_owned())?;
    let previously_allowed =
        SecKeychain::user_interaction_allowed().map_err(|_| "读取系统凭据失败。".to_owned())?;
    let _silent = if !allow_prompt && previously_allowed {
        Some(SecKeychain::disable_user_interaction().map_err(|_| "读取系统凭据失败。".to_owned())?)
    } else {
        None
    };
    // This guard restores true. Only create it when that was the prior value;
    // an already-disabled caller must remain disabled after this operation.
    operation()
}

#[cfg(all(test, target_os = "macos"))]
mod credential_interaction_tests {
    use super::*;
    use security_framework::os::macos::keychain::SecKeychain;

    #[test]
    fn silent_operations_restore_the_prior_interaction_policy_even_on_error() {
        let original = SecKeychain::user_interaction_allowed().unwrap();
        let result: Result<(), String> = with_credential_access(false, || {
            assert!(!SecKeychain::user_interaction_allowed().unwrap());
            Err("synthetic operation failure".into())
        });
        assert!(result.is_err());
        assert_eq!(SecKeychain::user_interaction_allowed().unwrap(), original);
        with_credential_access(true, || {
            assert_eq!(SecKeychain::user_interaction_allowed().unwrap(), original);
            Ok(())
        })
        .unwrap();
        let already_disabled = original.then(|| SecKeychain::disable_user_interaction().unwrap());
        with_credential_access(false, || Ok(())).unwrap();
        assert!(!SecKeychain::user_interaction_allowed().unwrap());
        drop(already_disabled);
        assert_eq!(SecKeychain::user_interaction_allowed().unwrap(), original);
    }
}

/// Read only item attributes. Never request password data or authentication UI.
pub fn credential_exists(id: &str) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        with_credential_access(false, || {
            use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
            let result = ItemSearchOptions::new()
                .class(ItemClass::generic_password())
                .service(CREDENTIAL_SERVICE)
                .account(id)
                .load_attributes(true)
                .skip_authenticated_items(true)
                .limit(Limit::Max(1))
                .search();
            match result {
                Ok(items) => Ok(!items.is_empty()),
                Err(error) if error.code() == -25300 => Ok(false),
                Err(_) => Err("无法检查系统钥匙串中的密钥状态。".into()),
            }
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Err("此平台的系统凭据存储尚未实现。".into())
    }
}

pub fn read_credential(id: &str, allow_prompt: bool) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        with_credential_access(allow_prompt, || {
            match security_framework::passwords::get_generic_password(CREDENTIAL_SERVICE, id) {
                Ok(bytes) => String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|_| "钥匙串中的密钥格式无效，请重新设置。".into()),
                Err(e) if e.code() == -25300 => Ok(None),
                Err(_) if !allow_prompt => Err("已保存的 API Key 需要系统钥匙串授权。".into()),
                Err(_) => Err("无法读取系统钥匙串，请解锁钥匙串或重新保存密钥。".into()),
            }
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (id, allow_prompt);
        Err("此平台的系统凭据存储尚未实现。".into())
    }
}

pub fn write_credential(id: &str, value: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        with_credential_access(true, || {
            let result = if let Some(value) = value {
                security_framework::passwords::set_generic_password(
                    CREDENTIAL_SERVICE,
                    id,
                    value.as_bytes(),
                )
            } else {
                security_framework::passwords::delete_generic_password(CREDENTIAL_SERVICE, id)
            };
            match result {
                Ok(()) => Ok(()),
                Err(e) if value.is_none() && e.code() == -25300 => Ok(()),
                Err(_) => Err("无法更新系统钥匙串；设置未保存。".into()),
            }
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (id, value);
        Err("此平台的系统凭据存储尚未实现。".into())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn permissions() -> Permissions {
    Permissions {
        accessibility: false,
        screen: false,
    }
}
#[cfg(not(target_os = "macos"))]
pub fn request_permission(_: &str) -> Result<(), String> {
    Err("此平台适配尚未实现。".into())
}
#[cfg(not(target_os = "macos"))]
pub fn copy_text(_: &str) -> Result<(), String> {
    Err("此平台适配尚未实现。".into())
}
#[cfg(not(target_os = "macos"))]
pub fn start(_: bool) -> mpsc::UnboundedReceiver<NativeEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(NativeEvent::Error(
        "此平台的取词与截图适配尚未实现。".into(),
    ));
    rx
}

#[cfg(not(target_os = "macos"))]
pub fn selection_target() -> Option<i32> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn start_for_target(screenshot: bool, _: Option<i32>) -> mpsc::UnboundedReceiver<NativeEvent> {
    start(screenshot)
}

#[cfg(not(target_os = "macos"))]
pub fn detect_language(_text: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn set_locale(_english: bool) {}

pub fn lookup_dictionary(text: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let input = std::ffi::CString::new(text).map_err(|_| "查询内容含无效字符。")?;
        let mut bytes = vec![0u8; 1024 * 1024];
        // SAFETY: the native helper copies at most bytes.len() bytes, retaining neither pointer.
        let len =
            unsafe { macos::ad_dictionary_lookup(input.as_ptr(), bytes.as_mut_ptr(), bytes.len()) };
        if len == 0 {
            return Err("系统词典未找到匹配词条。".into());
        }
        if len >= bytes.len() {
            return Err("词典释义过长。".into());
        }
        bytes.truncate(len);
        String::from_utf8(bytes).map_err(|_| "词典释义编码无效。".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        Err("此平台暂不支持系统词典。".into())
    }
}

pub fn open_dictionary(kind: &str, text: &str) -> Result<(), String> {
    if !matches!(kind, "system" | "youdao") || text.chars().count() > 512 {
        return Err("无效的词典查询。".into());
    }
    #[cfg(target_os = "macos")]
    {
        let text = std::ffi::CString::new(text).map_err(|_| "查询内容含无效字符。")?;
        // SAFETY: input is copied before the native helper dispatches the UI action.
        unsafe {
            macos::ad_open_dictionary(if kind == "system" { 0 } else { 1 }, text.as_ptr());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("此平台暂不支持词典入口。".into())
    }
}

pub fn lemma(text: &str) -> String {
    #[cfg(target_os = "macos")]
    if let Ok(input) = std::ffi::CString::new(text) {
        let mut bytes = vec![0u8; 2048];
        // SAFETY: helper copies bounded output and retains neither pointer.
        let len = unsafe { macos::ad_lemma(input.as_ptr(), bytes.as_mut_ptr(), bytes.len()) };
        if len > 0 && len < bytes.len() {
            bytes.truncate(len);
            if let Ok(value) = String::from_utf8(bytes) {
                return value;
            }
        }
    }
    text.trim().to_lowercase()
}

pub async fn choose_export(name: &str) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        macos::choose_export(name).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
        Err("此平台暂不支持导出。".into())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod dictionary_tests {
    #[test]
    fn missing_entry_does_not_claim_dictionaries_are_disabled() {
        assert_eq!(
            super::lookup_dictionary("yurelay_zxqv_unmatched_dictionary_probe_2026"),
            Err("系统词典未找到匹配词条。".into())
        );
    }
}
