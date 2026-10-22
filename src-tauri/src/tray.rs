use tauri::{AppHandle, tray::TrayIcon};

pub fn with_tray(app: &AppHandle, action: impl FnOnce(&TrayIcon) + Send + 'static) {
    let handle = app.clone();
    // Tauri's wrapper is Send, but its native tray uses Rc. Dispatch before
    // obtaining it so cloning, native updates and Drop all stay on main.
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = handle.tray_by_id("yurelay") {
            action(&tray);
        }
    });
}
