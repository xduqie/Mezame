mod config;
#[cfg(windows)]
mod lockscreen;
#[cfg(not(windows))]
mod lockscreen {
    pub fn apply(_: &std::path::Path, _: &crate::config::Config) -> Result<(), String> {
        Err("Lock-screen images are only supported on Windows".into())
    }
}
#[cfg(windows)]
mod winevents;

use chrono::{Datelike, Local, Timelike};
use config::Config;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

#[allow(dead_code)]
pub enum SysEvent {
    Lock,
    Unlock,
    DisplayOn,
    DisplayOff,
}

#[derive(Clone, Default, Serialize)]
pub struct PopupData {
    title: String,
    message: String,
    photo: Option<String>,
    dismiss_secs: u32,
    style: String,
    theme: String,
    font: String,
    accent: String,
    show_clock: bool,
    show_timer: bool,
    /// card width / 460
    scale: f64,
    /// transparent margin around the card, in px
    pad: f64,
}

#[derive(Clone, Copy)]
struct Geo {
    target: (i32, i32),
    enter: (i32, i32),
    exit: (i32, i32),
}

pub struct AppState {
    config: Mutex<Config>,
    popup: Mutex<Option<PopupData>>,
    geo: Mutex<Option<Geo>>,
    locked: AtomicBool,
    display_off: AtomicBool,
    pending_scheduled: AtomicBool,
}

// ---------------------------------------------------------------- triggers

/// Called from the Win32 listener thread. Must return quickly.
pub fn on_system_event(app: &AppHandle, ev: SysEvent) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    match ev {
        SysEvent::Lock => state.locked.store(true, Ordering::SeqCst),
        SysEvent::Unlock => {
            state.locked.store(false, Ordering::SeqCst);
            let pending = state.pending_scheduled.swap(false, Ordering::SeqCst);
            if cfg.popup_on_unlock || pending {
                schedule_popup(app, cfg.delay_ms, false);
            }
        }
        SysEvent::DisplayOff => state.display_off.store(true, Ordering::SeqCst),
        SysEvent::DisplayOn => {
            let was_off = state.display_off.swap(false, Ordering::SeqCst);
            if was_off && cfg.popup_on_display_wake {
                schedule_popup(app, cfg.delay_ms.max(1500), true);
            }
        }
    }
}

fn schedule_popup(app: &AppHandle, delay_ms: u64, only_if_unlocked: bool) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay_ms));
        if only_if_unlocked && app.state::<AppState>().locked.load(Ordering::SeqCst) {
            return;
        }
        build_popup(&app);
    });
}

/// The "auto popup reminder": checks the clock every few seconds.
fn start_scheduler(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_interval = Instant::now();
        let mut last_daily: Option<(i32, u32, u32)> = None;
        loop {
            std::thread::sleep(Duration::from_secs(10));
            let state = app.state::<AppState>();
            let cfg = state.config.lock().unwrap().clone();
            if !cfg.schedule_enabled {
                last_interval = Instant::now();
                continue;
            }
            let now = Local::now();
            let due = if cfg.schedule_mode == "daily" {
                let key = (now.year(), now.ordinal(), now.hour() * 60 + now.minute());
                let day_ok = cfg
                    .schedule_days
                    .contains(&(now.weekday().num_days_from_monday() as u8));
                if day_ok && now.format("%H:%M").to_string() == cfg.schedule_time && last_daily != Some(key) {
                    last_daily = Some(key);
                    true
                } else {
                    false
                }
            } else {
                let every = Duration::from_secs(cfg.schedule_interval_min.max(1) as u64 * 60);
                if last_interval.elapsed() >= every {
                    last_interval = Instant::now();
                    true
                } else {
                    false
                }
            };
            if due {
                if state.locked.load(Ordering::SeqCst) {
                    // Can't be seen behind the lock screen; show it right after unlock.
                    state.pending_scheduled.store(true, Ordering::SeqCst);
                } else {
                    build_popup(&app);
                }
            }
        }
    });
}

pub fn show_popup(app: &AppHandle) {
    let app = app.clone();
    // Build windows off the main thread: doing it in a sync command can deadlock on Windows.
    std::thread::spawn(move || build_popup(&app));
}

/// Returns (resting position, enter position, exit position, monitor scale factor), all physical px.
fn place(app: &AppHandle, pos: &str, win_w: f64, win_h: f64, pad: f64) -> ((i32, i32), (i32, i32), (i32, i32), f64) {
    let Some(m) = app.primary_monitor().ok().flatten() else {
        return ((100, 100), (-3000, 100), (3000, 100), 1.0);
    };
    let sf = m.scale_factor();
    let (mx, my) = (m.position().x, m.position().y);
    let (mw, mh) = (m.size().width as i32, m.size().height as i32);
    let (ww, wh) = ((win_w * sf).round() as i32, (win_h * sf).round() as i32);
    let p = (pad * sf).round() as i32;
    let margin = (24.0 * sf).round() as i32;
    let taskbar = (56.0 * sf).round() as i32;
    let (left, right) = (pos.contains("left"), pos.contains("right"));
    let (top, bottom) = (pos.contains("top"), pos.contains("bottom"));

    let x = if left {
        mx + margin - p
    } else if right {
        mx + mw - ww - margin + p
    } else {
        mx + (mw - ww) / 2
    };
    let y = if top {
        my + margin - p
    } else if bottom {
        my + mh - wh - margin - taskbar + p
    } else {
        my + (mh - wh) / 2
    };
    let (from_left, from_right) = (mx - ww, mx + mw);
    // Centered popups glide in from the left and leave to the right.
    let (enter_x, exit_x) = if left {
        (from_left, from_left)
    } else if right {
        (from_right, from_right)
    } else {
        (from_left, from_right)
    };
    ((x, y), (enter_x, y), (exit_x, y), sf)
}

fn glide(win: &WebviewWindow, from: (i32, i32), to: (i32, i32), ms: u64, ease_out: bool) {
    let step_ms = 8u64;
    let steps = (ms / step_ms).max(1);
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let e = if ease_out { 1.0 - (1.0 - t).powi(4) } else { t.powi(3) };
        let x = from.0 as f64 + (to.0 - from.0) as f64 * e;
        let y = from.1 as f64 + (to.1 - from.1) as f64 * e;
        let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        std::thread::sleep(Duration::from_millis(step_ms));
    }
}

fn build_popup(app: &AppHandle) {
    if app.get_webview_window("popup").is_some() {
        return;
    }
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let photo = cfg
        .photo_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .and_then(|p| config::photo_data_uri(p, 1000));

    let base = match cfg.size.as_str() {
        "compact" => 400.0,
        "large" => 560.0,
        _ => 460.0,
    };
    let scale = base / 460.0;
    let card_h = (if photo.is_some() { 540.0 } else { 300.0 }) * scale;
    let pad = if cfg.style == "funky" { 60.0 } else { 20.0 };
    let (win_w, win_h) = (base + pad * 2.0, card_h + pad * 2.0);

    let (target, enter, exit, sf) = place(app, &cfg.position, win_w, win_h, pad);
    let start = if cfg.style == "glide" { enter } else { target };
    *state.geo.lock().unwrap() = Some(Geo { target, enter, exit });
    *state.popup.lock().unwrap() = Some(PopupData {
        title: cfg.title,
        message: cfg.message,
        photo,
        dismiss_secs: cfg.dismiss_secs,
        style: cfg.style,
        theme: cfg.theme,
        font: cfg.font,
        accent: cfg.accent,
        show_clock: cfg.show_clock,
        show_timer: cfg.show_timer,
        scale,
        pad,
    });

    let built = WebviewWindowBuilder::new(app, "popup", WebviewUrl::App("popup.html".into()))
        .title("Mezame reminder")
        .inner_size(win_w, win_h)
        .position(start.0 as f64 / sf, start.1 as f64 / sf)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(true)
        .build();
    if let Err(e) = built {
        eprintln!("[mezame] could not open popup: {e}");
    }
}

fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
async fn save_config(app: AppHandle, state: State<'_, AppState>, config: Config) -> Result<String, String> {
    // Apply in memory first so popups use the new settings even if the disk write or lock screen step fails.
    *state.config.lock().unwrap() = config.clone();
    config::save(&app, &config)?;

    let mut notes: Vec<String> = Vec::new();
    if let Some(p) = config.photo_path.as_deref().filter(|p| !p.is_empty()) {
        if let Err(e) = config::photo_check(p) {
            notes.push(format!("the photo couldn't be read ({e})"));
        }
    }
    if config.set_lock_screen {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let cfg2 = config.clone();
        match tauri::async_runtime::spawn_blocking(move || lockscreen::apply(&dir, &cfg2)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => notes.push(format!("lock screen update failed ({e})")),
            Err(e) => notes.push(format!("lock screen update failed ({e})")),
        }
    }
    Ok(if notes.is_empty() { "Saved.".into() } else { format!("Saved, but {}.", notes.join("; ")) })
}

#[tauri::command]
fn trigger_popup(app: AppHandle) {
    show_popup(&app);
}

#[tauri::command]
fn get_popup_data(state: State<'_, AppState>) -> PopupData {
    state.popup.lock().unwrap().clone().unwrap_or_default()
}

/// The popup page calls this once it has rendered. "glide" popups start off-screen and slide in here.
#[tauri::command]
fn popup_ready(app: AppHandle, state: State<'_, AppState>) {
    let is_glide = state.popup.lock().unwrap().as_ref().map(|p| p.style == "glide").unwrap_or(false);
    let geo = *state.geo.lock().unwrap();
    if let (true, Some(g), Some(win)) = (is_glide, geo, app.get_webview_window("popup")) {
        std::thread::spawn(move || glide(&win, g.enter, g.target, 560, true));
    }
}

/// "minimal" and "funky" play their exit animation in CSS first; "glide" slides out here.
#[tauri::command]
fn close_popup(app: AppHandle, state: State<'_, AppState>) {
    let is_glide = state.popup.lock().unwrap().as_ref().map(|p| p.style == "glide").unwrap_or(false);
    let geo = *state.geo.lock().unwrap();
    let Some(win) = app.get_webview_window("popup") else { return };
    match (is_glide, geo) {
        (true, Some(g)) => {
            std::thread::spawn(move || {
                glide(&win, g.target, g.exit, 440, false);
                let _ = win.close();
            });
        }
        _ => {
            let _ = win.close();
        }
    }
}

#[tauri::command]
async fn photo_preview(path: String) -> Option<String> {
    config::photo_data_uri(&path, 480)
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let m = app.autolaunch();
    let on = m.is_enabled().unwrap_or(false);
    if enabled && !on {
        m.enable().map_err(|e| e.to_string())
    } else if !enabled && on {
        m.disable().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------- app

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let test = MenuItem::with_id(app, "test", "Preview popup", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &test, &quit])?;

    TrayIconBuilder::with_id("mezame-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Mezame")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "settings" => show_settings(app),
            "test" => show_popup(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                show_settings(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_settings(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            trigger_popup,
            get_popup_data,
            popup_ready,
            close_popup,
            photo_preview,
            get_autostart,
            set_autostart
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let (cfg, first_run) = config::load(&handle);
            app.manage(AppState {
                config: Mutex::new(cfg.clone()),
                popup: Mutex::new(None),
                geo: Mutex::new(None),
                locked: AtomicBool::new(false),
                display_off: AtomicBool::new(false),
                pending_scheduled: AtomicBool::new(false),
            });

            build_tray(&handle)?;

            if let Some(win) = app.get_webview_window("main") {
                let w = win.clone();
                win.on_window_event(move |e| {
                    if let WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
                if first_run {
                    let _ = win.show();
                }
            }

            #[cfg(windows)]
            winevents::start(handle.clone());
            start_scheduler(handle.clone());

            if cfg.set_lock_screen {
                if let Ok(dir) = handle.path().app_data_dir() {
                    std::thread::spawn(move || {
                        if let Err(e) = lockscreen::apply(&dir, &cfg) {
                            eprintln!("[mezame] lock screen: {e}");
                        }
                    });
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Mezame");
}
