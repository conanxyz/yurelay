//! Exercise the production window coordinator on AppKit, including transitions
//! without resize events. A hidden about:blank webview uses no user settings,
//! credentials, capture permissions, clipboard, shortcuts or network requests.
#[path = "../src/window_sizing.rs"]
mod window_sizing;
mod state {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use tauri::Manager;
    #[derive(Default)]
    pub struct AppState {
        pub sizing: Mutex<crate::window_sizing::WindowSizing>,
        pub failure: AtomicBool,
    }
    pub fn notice(app: &tauri::AppHandle, _: String) {
        app.state::<AppState>()
            .failure
            .store(true, Ordering::SeqCst);
    }
}
use std::sync::atomic::Ordering;
use tauri::Manager;
async fn until(mut check: impl FnMut() -> bool) -> Result<(), &'static str> {
    for _ in 0..60 {
        if check() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("native window state timed out")
}
async fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").unwrap();
    let initial = window_sizing::initial_size(
        &std::env::temp_dir().join("yurelay-no-saved-window-size"),
        [1440.0, 900.0],
    );
    if initial != [520.0, 560.0] {
        return Err("wrong default size".into());
    }
    let original = window
        .inner_size()
        .map_err(|_| "cannot read initial size")?;
    for _ in 0..3 {
        window_sizing::set_expanded(app, true)?;
        until(|| {
            app.state::<state::AppState>()
                .sizing
                .lock()
                .unwrap()
                .expanded
        })
        .await?;
        window_sizing::set_expanded(app, false)?;
        until(|| {
            let s = app.state::<state::AppState>();
            let sizing = s.sizing.lock().unwrap();
            sizing.settled()
        })
        .await?;
        if window
            .inner_size()
            .map_err(|_| "cannot read restored size")?
            != original
        {
            return Err("restore changed the normal window size".into());
        }
    }
    window
        .set_fullscreen(true)
        .map_err(|_| "cannot enter fullscreen")?;
    until(|| window.is_fullscreen().unwrap_or(false)).await?;
    window_sizing::set_expanded(app, false)?;
    until(|| {
        let s = app.state::<state::AppState>();
        let sizing = s.sizing.lock().unwrap();
        !window.is_fullscreen().unwrap_or(true) && sizing.settled()
    })
    .await?;
    app.state::<state::AppState>()
        .sizing
        .lock()
        .unwrap()
        .observe(false, true);
    window_sizing::set_expanded(app, true)?;
    until(|| {
        app.state::<state::AppState>()
            .sizing
            .lock()
            .unwrap()
            .expanded
    })
    .await?;
    window_sizing::set_expanded(app, false)?;
    until(|| {
        !app.state::<state::AppState>()
            .sizing
            .lock()
            .unwrap()
            .expanded
    })
    .await?;
    let s = app.state::<state::AppState>();
    let sizing = s.sizing.lock().unwrap();
    if !sizing.manual || !sizing.settled() {
        return Err("manual sizing was lost".into());
    }
    if s.failure.load(Ordering::SeqCst) {
        return Err("native transition failed".into());
    }
    Ok(())
}
fn main() {
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = "test.yurelay.window-sizing".into();
    let app = tauri::Builder::default()
        .manage(state::AppState::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .title("Yurelay window test")
            .visible(false)
            .inner_size(520.0, 560.0)
            .build()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let result = verify(&handle).await;
                println!(
                    "Native window sizing: passed={} detail={:?}",
                    result.is_ok(),
                    result.as_ref().err()
                );
                handle.exit(if result.is_ok() { 0 } else { 1 });
            });
            Ok(())
        })
        .build(context)
        .expect("native window test app builds");
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(30));
        eprintln!("Native window sizing watchdog timed out");
        std::process::exit(2);
    });
    std::process::exit(app.run_return(|_, _| {}));
}
