//! Windows has no supported way to draw a custom window on the lock screen, but
//! it does let an app set the lock-screen *picture*. We render the reminder
//! (photo + text card) into a screen-sized image and set it, so the reminder is
//! on screen the moment the display wakes.

use crate::config::Config;
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use image::{imageops::FilterType, DynamicImage, Rgba, RgbaImage};
use std::path::Path;

#[link(name = "user32")]
extern "system" {
    fn GetSystemMetrics(index: i32) -> i32;
}

pub fn apply(dir: &Path, cfg: &Config) -> Result<(), String> {
    let img = render(cfg)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join("lockscreen.png");
    DynamicImage::ImageRgba8(img)
        .to_rgb8()
        .save(&path)
        .map_err(|e| e.to_string())?;
    set_lock_screen(&path)
}

fn set_lock_screen(path: &Path) -> Result<(), String> {
    use windows::{
        core::HSTRING,
        Storage::StorageFile,
        System::UserProfile::LockScreen,
        Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
    };
    unsafe {
        // Ignore "already initialised" errors; we only need *an* apartment on this thread.
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let p = HSTRING::from(path.to_string_lossy().as_ref());
    let file = StorageFile::GetFileFromPathAsync(&p)
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;
    LockScreen::SetImageFileAsync(&file)
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())
}

fn screen_size() -> (u32, u32) {
    let (w, h) = unsafe { (GetSystemMetrics(0), GetSystemMetrics(1)) };
    if w >= 640 && h >= 480 {
        (w as u32, h as u32)
    } else {
        (1920, 1080)
    }
}

fn load_font(name: &str) -> Result<FontVec, String> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let p = Path::new(&windir).join("Fonts").join(name);
    let data = std::fs::read(&p).map_err(|e| format!("Can't read font {}: {e}", p.display()))?;
    FontVec::try_from_vec(data).map_err(|e| e.to_string())
}

fn background(cfg: &Config, w: u32, h: u32) -> RgbaImage {
    if let Some(p) = cfg.photo_path.as_deref().filter(|p| !p.is_empty()) {
        if let Ok(img) = image::open(p) {
            return img.resize_to_fill(w, h, FilterType::Triangle).to_rgba8();
        }
    }
    // Pre-dawn gradient when there is no photo.
    let mut img = RgbaImage::new(w, h);
    let (a, b) = ([20.0, 23.0, 43.0], [48.0, 58.0, 104.0]);
    for y in 0..h {
        let t = y as f32 / h as f32;
        let px = Rgba([
            (a[0] + (b[0] - a[0]) * t) as u8,
            (a[1] + (b[1] - a[1]) * t) as u8,
            (a[2] + (b[2] - a[2]) * t) as u8,
            255,
        ]);
        for x in 0..w {
            img.put_pixel(x, y, px);
        }
    }
    img
}

fn blend(p: &mut Rgba<u8>, c: [u8; 3], a: f32) {
    for i in 0..3 {
        p.0[i] = (p.0[i] as f32 * (1.0 - a) + c[i] as f32 * a).round() as u8;
    }
}

fn fill_round_rect(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, color: [u8; 3], alpha: f32) {
    let (iw, ih) = img.dimensions();
    let r = r.min(w / 2.0).min(h / 2.0);
    let x0 = x.floor().max(0.0) as u32;
    let y0 = y.floor().max(0.0) as u32;
    let x1 = ((x + w).ceil() as u32).min(iw);
    let y1 = ((y + h).ceil() as u32).min(ih);
    for py in y0..y1 {
        for px in x0..x1 {
            let lx = px as f32 + 0.5 - x;
            let ly = py as f32 + 0.5 - y;
            let cx = lx.clamp(r, w - r);
            let cy = ly.clamp(r, h - r);
            let d = ((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt();
            let cov = (r - d + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(img.get_pixel_mut(px, py), color, alpha * cov);
            }
        }
    }
}

fn wrap(font: &FontVec, size: f32, text: &str, max_w: f32) -> Vec<String> {
    let scaled = font.as_scaled(PxScale::from(size));
    let width = |s: &str| -> f32 { s.chars().map(|c| scaled.h_advance(scaled.glyph_id(c))).sum() };
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if cur.is_empty() || width(&cand) <= max_w {
                cur = cand;
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            }
        }
        lines.push(cur);
    }
    lines
}

fn draw_text(img: &mut RgbaImage, font: &FontVec, text: &str, size: f32, x: f32, baseline: f32, color: [u8; 3]) {
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let (w, h) = img.dimensions();
    let mut cx = x;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(cx, baseline));
        if let Some(og) = font.outline_glyph(glyph) {
            let b = og.px_bounds();
            og.draw(|gx, gy, cov| {
                let px = b.min.x as i32 + gx as i32;
                let py = b.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    blend(img.get_pixel_mut(px as u32, py as u32), color, cov);
                }
            });
        }
        cx += scaled.h_advance(id);
    }
}

fn render(cfg: &Config) -> Result<RgbaImage, String> {
    let (w, h) = screen_size();
    let regular = load_font("segoeui.ttf")?;
    let bold = load_font("segoeuib.ttf")?;
    let mut img = background(cfg, w, h);

    // Dim the picture a little so the card reads well on any photo.
    for p in img.pixels_mut() {
        for i in 0..3 {
            p.0[i] = (p.0[i] as f32 * 0.72) as u8;
        }
    }

    let (wf, hf) = (w as f32, h as f32);
    let pad = hf * 0.04;
    let card_w = (wf * 0.40).clamp(560.0, 980.0).min(wf * 0.9);
    let inner = card_w - pad * 2.0;
    let (t_size, b_size) = (hf * 0.050, hf * 0.028);

    let t_lines = wrap(&bold, t_size, &cfg.title, inner);
    let mut b_lines = wrap(&regular, b_size, &cfg.message, inner);
    if b_lines.len() > 8 {
        b_lines.truncate(8);
        if let Some(l) = b_lines.last_mut() {
            l.push_str("...");
        }
    }

    let (t_lh, b_lh) = (t_size * 1.25, b_size * 1.45);
    let (mark_h, gap1, gap2) = (hf * 0.006, hf * 0.022, hf * 0.018);
    let card_h = pad * 2.0 + mark_h + gap1 + t_lines.len() as f32 * t_lh + gap2 + b_lines.len() as f32 * b_lh;
    let cx = (wf - card_w) / 2.0;
    // Sit below the clock, above the notification area.
    let cy = (hf * 0.60 - card_h / 2.0).min(hf * 0.92 - card_h).max(hf * 0.30);

    fill_round_rect(&mut img, cx, cy, card_w, card_h, hf * 0.02, [17, 20, 43], 0.78);

    let x = cx + pad;
    let mut y = cy + pad;
    fill_round_rect(&mut img, x, y, hf * 0.06, mark_h, mark_h / 2.0, cfg.accent_rgb(), 1.0);
    y += mark_h + gap1;
    for l in &t_lines {
        draw_text(&mut img, &bold, l, t_size, x, y + t_size, [240, 242, 255]);
        y += t_lh;
    }
    y += gap2;
    for l in &b_lines {
        draw_text(&mut img, &regular, l, b_size, x, y + b_size, [201, 211, 242]);
        y += b_lh;
    }
    Ok(img)
}
