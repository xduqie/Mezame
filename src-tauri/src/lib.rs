pub mod config;
pub mod sys_events;

use std::sync::Mutex;
use std::time::Duration;
use tauri::{
    Manager, WebviewUrl, WebviewWindowBuilder,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[derive(Debug, Clone, Copy)]
pub enum SysEvent {
    Lock,
    Unlock,
    DisplayOff,
    DisplayOn,
}

struct AppState {
    cfg: Mutex<config::Config>,
}

fn show_popup(app: &tauri::AppHandle, cfg: &config::Config) {
    if let Some(w) = app.get_webview_window("popup") {
        let _ = w.close();
    }

    let photo_html = if let Some(path) = &cfg.photo_path {
        if let Some(uri) = config::photo_data_uri(path, 800) {
            format!(r#"<img src="{}" style="max-width: 100%; border-radius: 8px; margin-bottom: 15px;" />"#, uri)
        } else { String::new() }
    } else { String::new() };

    let dismiss_js = if cfg.dismiss_secs > 0 {
        format!("setTimeout(() => window.close(), {} * 1000);", cfg.dismiss_secs)
    } else { String::new() };

    let html = format!(
        r#"<!doctype html>
        <html>
        <head>
            <style>
                html, body {{ margin: 0; padding: 0; width: 100vw; height: 100vh; background: transparent !important; overflow: hidden; display: flex; align-items: center; justify-content: center; font-family: sans-serif; }}
                #box {{ background: rgba(30, 30, 46, 0.95); color: #cdd6f4; padding: 30px; border-radius: 16px; box-shadow: 0 8px 30px rgba(0,0,0,0.5); border: 1px solid #45475a; cursor: pointer; text-align: center; font-size: 1.2rem; max-width: 80%; display: flex; flex-direction: column; align-items: center; }}
                #box:hover {{ background: rgba(49, 50, 68, 0.98); }}
                h1 {{ font-size: 1.5rem; margin-top: 0; color: #89b4fa; }}
            </style>
        </head>
        <body onclick="window.close()">
            <div id="box">
                {}
                <h1>{}</h1>
                <p>{}</p>
            </div>
            <script>{}</script>
        </body>
        </html>"#,
        photo_html, cfg.title, cfg.message, dismiss_js
    );

    let url = format!("data:text/html;charset=utf-8,{}", urlencoding::encode(&html));

    WebviewWindowBuilder::new(app, "popup", WebviewUrl::External(url.parse().unwrap()))
        .title("")
        .inner_size(500.0, 400.0)
        .decorations(false)
        .transparent(true) 
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .center()
        .build()
        .expect("Failed to build popup window");
}

pub fn on_system_event(app: &tauri::AppHandle, ev: SysEvent) {
    let state = app.state::<AppState>();
    let cfg = state.cfg.lock().unwrap().clone();

    let should_popup = match ev {
        SysEvent::Unlock => cfg.popup_on_unlock,
        SysEvent::DisplayOn => cfg.popup_on_display_wake,
        _ => false,
    };

    if should_popup {
        let app_clone = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(cfg.delay_ms));
            let app_main = app_clone.clone();
            app_clone.run_on_main_thread(move || {
                show_popup(&app_main, &cfg);
            }).unwrap();
        });
    }
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> config::Config {
    state.cfg.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, state: tauri::State<AppState>, cfg: config::Config) {
    *state.cfg.lock().unwrap() = cfg.clone();
    let _ = config::save(&app, &cfg);
}

#[tauri::command]
fn trigger_popup(app: tauri::AppHandle, state: tauri::State<AppState>) {
    let cfg = state.cfg.lock().unwrap().clone();
    show_popup(&app, &cfg);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // ONLY store and autostart plugins remain
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            // Load config from disk
            let (cfg, _is_first_run) = config::load(app.handle());
            app.manage(AppState { cfg: Mutex::new(cfg) });

            // Start the raw Windows event listener!
            sys_events::start(app.handle().clone());

            // Handle Main Window Closing
            let main_window = app.get_webview_window("main").unwrap();
            let window = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            });

            // System Tray
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            trigger_popup
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mezame");
}