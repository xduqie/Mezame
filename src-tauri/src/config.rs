use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub title: String,
    pub message: String,
    pub photo_path: Option<String>,
    /// 0 = stay until clicked
    pub dismiss_secs: u32,
    pub delay_ms: u64,
    pub popup_on_unlock: bool,
    pub popup_on_display_wake: bool,
    pub set_lock_screen: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: "You're back".into(),
            message: "Take a breath. What did you sit down to do?".into(),
            photo_path: None,
            dismiss_secs: 12,
            delay_ms: 600,
            popup_on_unlock: true,
            popup_on_display_wake: true,
            set_lock_screen: false,
        }
    }
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// Returns (config, is_first_run).
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