use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_WIDTH: f32 = 320.0;
pub const DEFAULT_HEIGHT: f32 = 180.0;
pub const DEFAULT_FPS: u32 = 30;
pub const MIN_FPS: u32 = 1;
pub const MAX_FPS: u32 = 60;
const MARGIN: f32 = 16.0;

/// 擷取範圍，以主螢幕實體像素表示（0,0 = 螢幕左上角）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Config {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub fps: u32,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
    #[serde(default)]
    pub grayscale: bool,
    /// `None` 表示擷取整個主螢幕。
    #[serde(default)]
    pub capture_region: Option<CaptureRegion>,
    /// 顯示旋轉角度（弧度）。
    #[serde(default)]
    pub rotation: f32,
}

impl Default for Config {
    fn default() -> Self {
        // 預設放在工作區右下角，避免被工作列蓋住；查不到工作區時改用整個螢幕。
        let (x, y) = match crate::monitor::primary_monitor_work_area_logical() {
            Some(area) => (
                (area.right() - DEFAULT_WIDTH - MARGIN).max(area.left()),
                (area.bottom() - DEFAULT_HEIGHT - MARGIN).max(area.top()),
            ),
            None => {
                let (screen_w, screen_h) = crate::monitor::primary_monitor_size_logical();
                (
                    (screen_w - DEFAULT_WIDTH - MARGIN).max(0.0),
                    (screen_h - DEFAULT_HEIGHT - MARGIN).max(0.0),
                )
            }
        };
        Self {
            x,
            y,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            fps: DEFAULT_FPS,
            flip_x: false,
            flip_y: false,
            grayscale: false,
            capture_region: None,
            rotation: 0.0,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("mini_screen");
    Some(dir.join("config.json"))
}
