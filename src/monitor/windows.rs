//! Win32 螢幕幾何／DPI 查詢。`windows_capture` 的 `Monitor` 不提供位置與 DPI，
//! 因此取其 `HMONITOR` 直接呼叫 `GetMonitorInfoW`/`GetDpiForMonitor`。

use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

fn primary_monitor() -> Option<windows_capture::monitor::Monitor> {
    windows_capture::monitor::Monitor::primary().ok()
}

fn monitor_info(monitor: &windows_capture::monitor::Monitor) -> Option<MONITORINFO> {
    let hmonitor = monitor.as_raw_hmonitor() as HMONITOR;
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { GetMonitorInfoW(hmonitor, &mut info) } == 0 {
        return None;
    }
    Some(info)
}

pub(super) fn raw_rect_physical() -> Option<(i32, i32, u32, u32)> {
    let info = monitor_info(&primary_monitor()?)?;
    let rect = info.rcMonitor;
    Some((
        rect.left,
        rect.top,
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    ))
}

/// DPI 縮放比例（1.0 = 96 DPI），失敗時為 1.0。
pub(super) fn raw_scale_factor() -> f32 {
    let Some(monitor) = primary_monitor() else {
        return 1.0;
    };
    let hmonitor = monitor.as_raw_hmonitor() as HMONITOR;
    let mut dpi_x = 0u32;
    let mut dpi_y = 0u32;
    let ok = unsafe { GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) } == 0;
    if ok && dpi_x > 0 {
        dpi_x as f32 / 96.0
    } else {
        1.0
    }
}

pub(super) fn raw_work_area_physical() -> Option<(i32, i32, i32, i32)> {
    let info = monitor_info(&primary_monitor()?)?;
    let work = info.rcWork;
    Some((work.left, work.top, work.right, work.bottom))
}
