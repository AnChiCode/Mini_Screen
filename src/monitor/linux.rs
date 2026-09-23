//! 基於 `xcap` 的螢幕幾何查詢。

use xcap::Monitor;

fn primary_monitor() -> Option<Monitor> {
    Monitor::all()
        .ok()?
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
}

pub(super) fn raw_rect_physical() -> Option<(i32, i32, u32, u32)> {
    let monitor = primary_monitor()?;
    Some((
        monitor.x().ok()?,
        monitor.y().ok()?,
        monitor.width().ok()?,
        monitor.height().ok()?,
    ))
}

/// 失敗時為 1.0。
pub(super) fn raw_scale_factor() -> f32 {
    primary_monitor()
        .and_then(|m| m.scale_factor().ok())
        .filter(|scale| *scale > 0.0)
        .unwrap_or(1.0)
}

/// Linux 沒有跨桌面環境的工作區查詢，一律回傳 `None`。
pub(super) fn raw_work_area_physical() -> Option<(i32, i32, i32, i32)> {
    None
}
