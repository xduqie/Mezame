use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // content
    pub title: String,
    pub message: String,
    pub photo_path: Option<String>,
    // triggers
    pub popup_on_unlock: bool,
    pub popup_on_display_wake: bool,
    pub set_lock_screen: bool,
    pub delay_ms: u64,
    /// 0 = stay until clicked
    pub dismiss_secs: u32,
    // look
    /// "minimal" | "glide" | "funky"
    pub style: String,
    /// "dark" | "light"
    pub theme: String,
    /// "serif" | "sans" | "mono"
    pub font: String,
    pub accent: String,
    /// "compact" | "regular" | "large"
    pub size: String,
    /// "center" | "top-left" | "top-right" | "bottom-left" | "bottom-right"
    pub position: String,
    pub show_clock: bool,
    pub show_timer: bool,
    // schedule
    pub schedule_enabled: bool,
    /// "interval" | "daily"
    pub schedule_mode: String,
    pub schedule_interval_min: u32,
    /// "HH:MM" (24h)
    pub schedule_time: String,
    /// 0 = Monday .. 6 = Sunday
    pub schedule_days: Vec<u8>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: "You're back".into(),
            message: "Take a breath. What did you sit down to do?".into(),
            photo_path: None,
            popup_on_unlock: true,
            popup_on_display_wake: true,
            set_lock_screen: false,
            delay_ms: 600,
            dismiss_secs: 12,
            style: "minimal".into(),
            theme: "dark".into(),
            font: "serif".into(),
            accent: "#ffc933".into(),
            size: "regular".into(),
            position: "center".into(),
            show_clock: true,
            show_timer: true,
            schedule_enabled: false,
            schedule_mode: "interval".into(),
            schedule_interval_min: 60,
            schedule_time: "09:00".into(),
            schedule_days: vec![0, 1, 2, 3, 4, 5, 6],
        }
    }
}

impl Config {
    #[allow(dead_code)]
    pub fn accent_rgb(&self) -> [u8; 3] {
        let h = self.accent.trim_start_matches('#');
        if h.len() == 6 {
            if let Ok(n) = u32::from_str_radix(h, 16) {
                return [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            }
        }
        [255, 201, 51]
    }
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// Returns (config, is_first_run). Old settings files keep working: missing fields get defaults.
pub fn load(app: &AppHandle) -> (Config, bool) {
    match path(app).ok().and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(txt) => (serde_json::from_str(&txt).unwrap_or_default(), false),
        None => (Config::default(), true),
    }
}

pub fn save(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path(app)?, json).map_err(|e| e.to_string())
}

/// Reads an image, shrinks it to fit `max` px, and returns a JPEG data URI.
pub fn photo_data_uri(path: &str, max: u32) -> Option<String> {
    let img = image::open(path).ok()?.thumbnail(max, max);
    let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
    let mut buf = Vec::new();
    rgb.write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg).ok()?;
    Some(format!("data:image/jpeg;base64,{}", STANDARD.encode(buf)))
}

/// Used on save to tell the user straight away if a photo can't be decoded.
pub fn photo_check(path: &str) -> Result<(), String> {
    image::open(path).map(|_| ()).map_err(|e| e.to_string())
}
