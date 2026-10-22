//! Runs the production tray boundary against a real macOS main event loop.
//! No user database, credentials, capture, shortcuts or webviews are opened.
#[path = "../src/tray.rs"]
mod tray;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tauri::tray::TrayIconBuilder;

fn main() {
    let main_thread = std::thread::current().id();
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            TrayIconBuilder::with_id("yurelay")
                .title("Yurelay test")
                .build(app)?;
            let app = app.handle().clone();
            std::thread::spawn(move || {
                let wrong_thread = Arc::new(AtomicBool::new(false));
                let updates = Arc::new(AtomicUsize::new(0));
                let observed = updates.clone();
                let wrong = wrong_thread.clone();
                tray::with_tray(&app, move |tray| {
                    wrong.store(std::thread::current().id() != main_thread, Ordering::SeqCst);
                    // Updating the native title also exercises Tauri's own
                    // clone/drop path, as capture selection does in the app.
                    tray.set_title(Some("Yurelay test complete")).unwrap();
                    observed.fetch_add(1, Ordering::SeqCst);
                });
                let workers: Vec<_> = (0..8)
                    .map(|_| {
                        let app = app.clone();
                        let updates = updates.clone();
                        let wrong_thread = wrong_thread.clone();
                        std::thread::spawn(move || {
                            for _ in 0..64 {
                                let updates = updates.clone();
                                let wrong_thread = wrong_thread.clone();
                                tray::with_tray(&app, move |tray| {
                                    if std::thread::current().id() != main_thread {
                                        wrong_thread.store(true, Ordering::SeqCst);
                                    }
                                    tray.set_title(Some("Yurelay tray stress")).unwrap();
                                    updates.fetch_add(1, Ordering::SeqCst);
                                });
                            }
                        })
                    })
                    .collect();
                for worker in workers {
                    worker.join().unwrap();
                }
                let item = tauri::menu::MenuItem::with_id(
                    &app,
                    "test-settings",
                    "Synthetic settings",
                    true,
                    None::<&str>,
                )
                .unwrap();
                let menu = tauri::menu::Menu::with_items(&app, &[&item]).unwrap();
                let observed = updates.clone();
                let wrong = wrong_thread.clone();
                tray::with_tray(&app, move |tray| {
                    if std::thread::current().id() != main_thread {
                        wrong.store(true, Ordering::SeqCst);
                    }
                    tray.set_menu(Some(menu)).unwrap();
                    observed.fetch_add(1, Ordering::SeqCst);
                });
                let finish = app.clone();
                app.run_on_main_thread(move || {
                    let count = updates.load(Ordering::SeqCst);
                    let passed = count == 514 && !wrong_thread.load(Ordering::SeqCst);
                    println!("Native tray thread affinity: updates={count} passed={passed}");
                    drop(finish.remove_tray_by_id("yurelay"));
                    finish.exit(if passed { 0 } else { 1 });
                })
                .unwrap();
            });
            Ok(())
        })
        .build(context)
        .expect("native tray test app builds");
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(30));
        eprintln!("Native tray thread affinity test timed out");
        std::process::exit(2);
    });
    std::process::exit(app.run_return(|_, _| {}));
}
