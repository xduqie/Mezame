use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    Manager, WebviewUrl, WebviewWindowBuilder, Listener,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PopupConfig {
    message: String,
    delay_secs: u64,
    auto_dismiss_secs: u64,
    idle_threshold_secs: u64,
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            message: "Welcome back!".into(),
            delay_secs: 2,
            auto_dismiss_secs: 5,
            idle_threshold_secs: 60,
        }
    }
}

struct AppState {
    popup_config: Mutex<PopupConfig>,
    last_idle_start: Mutex<Option<Instant>>,
}

fn show_popup(app: &tauri::AppHandle, config: &PopupConfig) {
    if let Some(w) = app.get_webview_window("popup") {
        let _ = w.close();
    }

    let encoded_msg = urlencoding::encode(&config.message);
    
    // CHANGED: Pointing to popup.html (from the public folder)
    let url = format!(
        "popup.html?message={}&dismiss={}",
        encoded_msg, config.auto_dismiss_secs
    );

    WebviewWindowBuilder::new(app, "popup", WebviewUrl::App(url.into()))
        .title("")
        .inner_size(400.0, 200.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .center()
        .build()
        .expect("Failed to build popup window");
}

#[tauri::command]
fn get_popup_config(state: tauri::State<AppState>) -> PopupConfig {
    state.popup_config.lock().unwrap().clone()
}

#[tauri::command]
fn set_popup_config(state: tauri::State<AppState>, config: PopupConfig) {
    *state.popup_config.lock().unwrap() = config;
}

#[tauri::command]
fn trigger_popup(app: tauri::AppHandle, state: tauri::State<AppState>) {
    let config = state.popup_config.lock().unwrap().clone();
    show_popup(&app, &config);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Set the plugin threshold to 1 second so we get instant updates
        .plugin(tauri_plugin_idlemonitor::Builder::new().idle_threshold_secs(1).build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(AppState {
            popup_config: Mutex::new(PopupConfig::default()),
            last_idle_start: Mutex::new(None),
        })
        .setup(|app| {
            // 1. Handle Main Window Closing
            let main_window = app.get_webview_window("main").unwrap();
            let window = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            });

            // 2. System Tray Setup
            let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Mezame")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            // 3. IDLE MONITOR LOGIC
            let handle = app.handle().clone();
            handle.clone().listen_any("idle", move |event| {
                // ADDED: Debug statement to see if the event fires
                println!("DEBUG: Idle Event Fired! Payload: {}", event.payload());
                
                let payload = event.payload();
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(payload) {
                    if let Some(is_idle) = parsed.get("idle").and_then(|v| v.as_bool()) {
                        let state = handle.state::<AppState>();
                        if is_idle {
                            // User went idle
                            let mut start = state.last_idle_start.lock().unwrap();
                            if start.is_none() {
                                *start = Some(Instant::now());
                            }
                        } else {
                            // User returned
                            let mut start = state.last_idle_start.lock().unwrap();
                            if let Some(idle_start_time) = start.take() {
                                let idle_duration = idle_start_time.elapsed().as_secs();
                                let config = state.popup_config.lock().unwrap().clone();
                                println!("User returned after {} seconds", idle_duration);

                                if idle_duration >= config.idle_threshold_secs {
                                    let h = handle.clone();
                                    let cfg = config.clone();
                                    std::thread::spawn(move || {
                                        std::thread::sleep(Duration::from_secs(cfg.delay_secs));
                                        show_popup(&h, &cfg);
                                    });
                                }
                            }
                        }
                    }
                }
            });

            // 4. RESUME FROM SLEEP LOGIC
            let handle2 = app.handle().clone();
            handle2.clone().listen_any("resume", move |_| {
                println!("DEBUG: System Resume Event Fired!");
                let state = handle2.state::<AppState>();
                let config = state.popup_config.lock().unwrap().clone();
                let h = handle2.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(config.delay_secs));
                    show_popup(&h, &config);
                });
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_popup_config,
            set_popup_config,
            trigger_popup
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mezame");
}