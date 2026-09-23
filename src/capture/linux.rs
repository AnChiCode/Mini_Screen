//! Linux 擷取後端：在輪詢迴圈中呼叫 `xcap` 的同步擷取。
//!
//! Wayland 用戶端無法得知自身視窗位置（`outer_rect` 恆為 `None`），因此不支援背景擷取：
//! `set_background_region` 不做事、`take_latest_background_frame` 恆為 `None`，
//! UI 會退回純色毛玻璃填充。

use image::RgbaImage;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use xcap::Monitor;

use super::{MAX_CAPTURE_DIM, apply_grayscale, downscale};
use crate::config::CaptureRegion;

/// 單次休眠上限，讓 fps 變更或關閉能迅速生效。
const MAX_SLEEP_SLICE: Duration = Duration::from_millis(15);

struct SharedState {
    frame: Mutex<Option<RgbaImage>>,
    fps: AtomicU32,
    grayscale: AtomicBool,
    region: Mutex<Option<CaptureRegion>>,
    running: AtomicBool,
    /// 視窗最小化時為 true，暫停擷取。
    paused: AtomicBool,
}

pub struct CaptureHandle {
    shared: Arc<SharedState>,
    thread: Option<JoinHandle<()>>,
}

impl CaptureHandle {
    pub fn start(
        initial_fps: u32,
        initial_grayscale: bool,
        initial_region: Option<CaptureRegion>,
    ) -> Self {
        let shared = Arc::new(SharedState {
            frame: Mutex::new(None),
            fps: AtomicU32::new(initial_fps.max(1)),
            grayscale: AtomicBool::new(initial_grayscale),
            region: Mutex::new(initial_region),
            running: AtomicBool::new(true),
            paused: AtomicBool::new(false),
        });

        let thread = std::thread::spawn({
            let shared = shared.clone();
            move || capture_loop(&shared)
        });

        Self {
            shared,
            thread: Some(thread),
        }
    }

    pub fn set_fps(&self, fps: u32) {
        self.shared.fps.store(fps.max(1), Ordering::Relaxed);
    }

    pub fn set_grayscale(&self, enabled: bool) {
        self.shared.grayscale.store(enabled, Ordering::Relaxed);
    }

    /// 暫停或恢復擷取（視窗最小化時使用）。
    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Relaxed);
    }

    pub fn set_region(&self, region: Option<CaptureRegion>) {
        if let Ok(mut guard) = self.shared.region.lock() {
            *guard = region;
        }
    }

    /// 此後端不支援，不做事。
    pub fn set_background_region(&self, _region: Option<CaptureRegion>) {}

    /// 取出上次呼叫後的最新畫面。
    pub fn take_latest_frame(&self) -> Option<RgbaImage> {
        self.shared
            .frame
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    /// 此後端不支援，恆為 `None`。
    pub fn take_latest_background_frame(&self) -> Option<RgbaImage> {
        None
    }

    /// 擷取一張未經處理的完整螢幕畫面，供區域選取器當背景。
    pub fn snapshot(&self) -> Option<RgbaImage> {
        primary_monitor()?.capture_image().ok()
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn primary_monitor() -> Option<Monitor> {
    Monitor::all()
        .ok()?
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
}

fn capture_loop(shared: &SharedState) {
    while shared.running.load(Ordering::Relaxed) {
        let start = Instant::now();

        if shared.paused.load(Ordering::Relaxed) {
            std::thread::sleep(MAX_SLEEP_SLICE);
            continue;
        }

        // 每幀重新取得主螢幕，以反映解析度／主螢幕變更；取不到就跳過這一幀。
        if let Some(monitor) = primary_monitor() {
            let monitor_width = monitor.width().unwrap_or(1920).max(1);
            let monitor_height = monitor.height().unwrap_or(1080).max(1);
            let is_grayscale = shared.grayscale.load(Ordering::Relaxed);

            let current_region = shared.region.lock().ok().and_then(|guard| *guard);
            let captured = match current_region {
                Some(r) => {
                    let (x, y, width, height) = clamp_region(r, monitor_width, monitor_height);
                    monitor.capture_region(x, y, width, height)
                }
                None => monitor.capture_image(),
            };
            if let Ok(image) = captured {
                let mut image = downscale(image, MAX_CAPTURE_DIM);
                if is_grayscale {
                    apply_grayscale(&mut image);
                }
                if let Ok(mut guard) = shared.frame.lock() {
                    *guard = Some(image);
                }
            }
        }

        // 分段休眠並每次重讀 fps，讓等待中的 fps 變更能立即生效。
        loop {
            if !shared.running.load(Ordering::Relaxed) {
                return;
            }
            let target =
                Duration::from_secs_f64(1.0 / shared.fps.load(Ordering::Relaxed).max(1) as f64);
            let elapsed = start.elapsed();
            if elapsed >= target {
                break;
            }
            std::thread::sleep((target - elapsed).min(MAX_SLEEP_SLICE));
        }
    }
}

/// 將擷取範圍限制在螢幕內（`xcap` 遇到越界範圍會直接報錯）。回傳 `(x, y, width, height)`。
fn clamp_region(r: CaptureRegion, monitor_width: u32, monitor_height: u32) -> (u32, u32, u32, u32) {
    let x = r.x.min(monitor_width - 1);
    let y = r.y.min(monitor_height - 1);
    let width = r.width.min(monitor_width - x).max(1);
    let height = r.height.min(monitor_height - y).max(1);
    (x, y, width, height)
}
