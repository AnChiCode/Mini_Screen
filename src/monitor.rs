//! 主螢幕查詢，以及實體像素與 egui 邏輯點之間的轉換。
//!
//! 螢幕幾何以實體像素回報，視窗幾何（含 `Config::x/y/width/height`）則是邏輯點，
//! 兩者比較前必須先轉換。各平台的實際查詢在 `platform::raw_*`。

use egui::{Pos2, Rect, Vec2};

use crate::config::CaptureRegion;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

/// 主螢幕的 `(x, y, width, height)`（實體像素）。刻意不快取，以反映解析度／DPI／主螢幕變更。
pub fn primary_monitor_rect_physical() -> Option<(i32, i32, u32, u32)> {
    platform::raw_rect_physical()
}

/// 主螢幕尺寸（實體像素）。
pub fn primary_monitor_dimensions() -> Option<(u32, u32)> {
    let (_, _, width, height) = primary_monitor_rect_physical()?;
    Some((width, height))
}

/// 主螢幕邊界（實體像素）。
pub fn primary_monitor_bounds() -> Option<Rect> {
    let (x, y, width, height) = primary_monitor_rect_physical()?;
    Some(Rect::from_min_size(
        Pos2::new(x as f32, y as f32),
        Vec2::new(width as f32, height as f32),
    ))
}

/// 主螢幕邊界（egui 邏輯點）。
pub fn primary_monitor_bounds_logical(pixels_per_point: f32) -> Option<Rect> {
    let bounds = primary_monitor_bounds()?;
    Some(Rect::from_min_size(
        Pos2::new(
            bounds.min.x / pixels_per_point,
            bounds.min.y / pixels_per_point,
        ),
        Vec2::new(
            bounds.width() / pixels_per_point,
            bounds.height() / pixels_per_point,
        ),
    ))
}

/// 主螢幕尺寸（egui 邏輯點）。用於建立預設 `Config`，此時還沒有 egui context，
/// 所以直接向作業系統查詢縮放比例。
pub fn primary_monitor_size_logical() -> (f32, f32) {
    let Some((_, _, width, height)) = primary_monitor_rect_physical() else {
        return (1920.0, 1080.0);
    };
    let scale = platform::raw_scale_factor();
    let scale = if scale > 0.0 { scale } else { 1.0 };
    (width as f32 / scale, height as f32 / scale)
}

/// 主螢幕的工作區（扣除工作列，egui 邏輯點），用於預設 `Config` 位置。
/// 平台不支援時回傳 `None`。
pub fn primary_monitor_work_area_logical() -> Option<Rect> {
    let (left, top, right, bottom) = platform::raw_work_area_physical()?;
    let scale = platform::raw_scale_factor();
    let scale = if scale > 0.0 { scale } else { 1.0 };
    Some(Rect::from_min_max(
        Pos2::new(left as f32 / scale, top as f32 / scale),
        Pos2::new(right as f32 / scale, bottom as f32 / scale),
    ))
}

/// 將視窗的邏輯點矩形轉換為限制在主螢幕內的 `CaptureRegion`，
/// 用來擷取視窗正後方的桌面。
pub fn window_footprint(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    pixels_per_point: f32,
) -> Option<CaptureRegion> {
    let bounds = primary_monitor_bounds()?;
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return None;
    }

    let win_x = x * pixels_per_point;
    let win_y = y * pixels_per_point;
    let win_w = (width * pixels_per_point).max(1.0);
    let win_h = (height * pixels_per_point).max(1.0);

    let rel_x = (win_x - bounds.min.x).clamp(0.0, bounds.width());
    let rel_y = (win_y - bounds.min.y).clamp(0.0, bounds.height());
    let w = win_w.min(bounds.width() - rel_x).max(1.0);
    let h = win_h.min(bounds.height() - rel_y).max(1.0);

    Some(CaptureRegion {
        x: rel_x.round() as u32,
        y: rel_y.round() as u32,
        width: w.round() as u32,
        height: h.round() as u32,
    })
}

/// 限制 `pos`，讓尺寸為 `size` 的視窗留在主螢幕內，且距各邊至少 `margin`（皆為邏輯點）。
pub fn clamp_to_screen(pos: Pos2, size: Vec2, margin: f32, pixels_per_point: f32) -> Pos2 {
    let Some(bounds) = primary_monitor_bounds_logical(pixels_per_point) else {
        return pos;
    };
    let max_x = (bounds.right() - size.x - margin).max(bounds.left() + margin);
    let max_y = (bounds.bottom() - size.y - margin).max(bounds.top() + margin);
    Pos2::new(
        pos.x.clamp(bounds.left() + margin, max_x),
        pos.y.clamp(bounds.top() + margin, max_y),
    )
}
