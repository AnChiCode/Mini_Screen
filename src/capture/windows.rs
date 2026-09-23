use image::RgbaImage;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::{Frame, FrameBuffer};
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use super::{MAX_CAPTURE_DIM, apply_grayscale, downscale};
use crate::config::CaptureRegion;

/// 監督執行緒與各個 `MirrorCapture` 工作階段共享的狀態；工作階段可能重建，所以不放在處理器上。
#[derive(Clone)]
struct SharedState {
    frame: Arc<Mutex<Option<RgbaImage>>>,
    background_frame: Arc<Mutex<Option<RgbaImage>>>,
    fps: Arc<AtomicU32>,
    grayscale: Arc<AtomicBool>,
    region: Arc<Mutex<Option<CaptureRegion>>>,
    background_region: Arc<Mutex<Option<CaptureRegion>>>,
    running: Arc<AtomicBool>,
    /// 視窗最小化時為 true，暫停處理畫面。
    paused: Arc<AtomicBool>,
    /// 待處理的 `CaptureHandle::snapshot` 請求。
    snapshot_request: Arc<Mutex<Option<mpsc::Sender<RgbaImage>>>>,
}

/// 在背景執行緒以可調 fps 擷取螢幕，透過共享槽位把最新畫面交給 UI。
///
/// 每幀從同一張畫面裁出兩張影像：鏡像內容（`region`，或整個螢幕），
/// 以及視窗自身後方的背景（`background_region`）。
pub struct CaptureHandle {
    shared: SharedState,
    thread: Option<JoinHandle<()>>,
}

impl CaptureHandle {
    pub fn start(
        initial_fps: u32,
        initial_grayscale: bool,
        initial_region: Option<CaptureRegion>,
    ) -> Self {
        let shared = SharedState {
            frame: Arc::new(Mutex::new(None)),
            background_frame: Arc::new(Mutex::new(None)),
            fps: Arc::new(AtomicU32::new(initial_fps.max(1))),
            grayscale: Arc::new(AtomicBool::new(initial_grayscale)),
            region: Arc::new(Mutex::new(initial_region)),
            background_region: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(true)),
            paused: Arc::new(AtomicBool::new(false)),
            snapshot_request: Arc::new(Mutex::new(None)),
        };

        let thread = std::thread::spawn({
            let shared = shared.clone();
            move || supervisor_loop(shared)
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

    /// 設定視窗目前所在的螢幕區域，用來裁切背景。
    pub fn set_background_region(&self, region: Option<CaptureRegion>) {
        if let Ok(mut guard) = self.shared.background_region.lock() {
            *guard = region;
        }
    }

    /// 取出上次呼叫後的最新內容畫面。
    pub fn take_latest_frame(&self) -> Option<RgbaImage> {
        self.shared
            .frame
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    /// 取出上次呼叫後的最新背景畫面。
    pub fn take_latest_background_frame(&self) -> Option<RgbaImage> {
        self.shared
            .background_frame
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    /// 擷取一張未經處理的完整螢幕畫面，供區域選取器當背景。
    ///
    /// 直接取用現有工作階段的下一幀；`windows_capture` 沒有同步的單次擷取。
    pub fn snapshot(&self) -> Option<RgbaImage> {
        let (tx, rx) = mpsc::channel();
        *self.shared.snapshot_request.lock().ok()? = Some(tx);
        rx.recv_timeout(Duration::from_secs(2)).ok()
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

/// 管理擷取工作階段：啟動主螢幕擷取並等待其結束（應用程式關閉，或螢幕被拔除／變更），
/// 之後重新取得主螢幕再啟動。
fn supervisor_loop(shared: SharedState) {
    // 隱藏游標需 Windows 10 2004+，隱藏擷取邊框需 Windows 11 22H2+，不支援時會直接建立失敗；
    // 因此第一次失敗後就把兩者降級為 `Default`。
    let mut cursor_setting = CursorCaptureSettings::WithoutCursor;
    let mut border_setting = DrawBorderSettings::WithoutBorder;
    let mut degraded = false;

    while shared.running.load(Ordering::Relaxed) {
        let Ok(monitor) = Monitor::primary() else {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        };

        let settings = Settings::new(
            monitor,
            cursor_setting,
            border_setting,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            shared.clone(),
        );

        match MirrorCapture::start_free_threaded(settings) {
            Ok(control) => {
                let _ = control.wait();
            }
            Err(_) if !degraded => {
                cursor_setting = CursorCaptureSettings::Default;
                border_setting = DrawBorderSettings::Default;
                degraded = true;
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

/// 主螢幕的擷取處理器。畫面到達時機不由我們控制，因此以丟棄過早到達的畫面來實現 fps 限制。
struct MirrorCapture {
    shared: SharedState,
    last_capture: Instant,
}

impl GraphicsCaptureApiHandler for MirrorCapture {
    type Flags = SharedState;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            shared: ctx.flags,
            // 讓第一張畫面立即被處理。
            last_capture: Instant::now() - Duration::from_secs(1),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if !self.shared.running.load(Ordering::Relaxed) {
            capture_control.stop();
            return Ok(());
        }

        if self.shared.paused.load(Ordering::Relaxed) {
            return Ok(());
        }

        // 快照請求不受 fps 限制，下一幀就回應。
        if let Some(tx) = self
            .shared
            .snapshot_request
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
            && let Ok(mut buffer) = frame.buffer()
            && let Some(image) = rgba_image_from_buffer(&mut buffer)
        {
            let _ = tx.send(image);
        }

        let target =
            Duration::from_secs_f64(1.0 / self.shared.fps.load(Ordering::Relaxed).max(1) as f64);
        if self.last_capture.elapsed() < target {
            return Ok(());
        }
        self.last_capture = Instant::now();

        let frame_width = frame.width();
        let frame_height = frame.height();
        let is_grayscale = self.shared.grayscale.load(Ordering::Relaxed);

        let current_region = self.shared.region.lock().ok().and_then(|guard| *guard);
        match current_region {
            Some(r) => {
                let (x, y, width, height) = clamp_region(r, frame_width, frame_height);
                if let Ok(mut buffer) = frame.buffer_crop(x, y, x + width, y + height) {
                    store_captured_frame(&mut buffer, is_grayscale, &self.shared.frame);
                }
            }
            None => {
                if let Ok(mut buffer) = frame.buffer() {
                    store_captured_frame(&mut buffer, is_grayscale, &self.shared.frame);
                }
            }
        }

        // 另外裁出視窗後方的背景，與鏡像的 `region` 無關。
        let current_background_region = self
            .shared
            .background_region
            .lock()
            .ok()
            .and_then(|guard| *guard);
        if let Some(r) = current_background_region {
            let (x, y, width, height) = clamp_region(r, frame_width, frame_height);
            if let Ok(mut buffer) = frame.buffer_crop(x, y, x + width, y + height) {
                store_captured_frame(&mut buffer, is_grayscale, &self.shared.background_frame);
            }
        }

        Ok(())
    }
}

/// 將緩衝區轉為 `RgbaImage`（縮小、視需要轉灰階）後存入 `slot`；讀取失敗時保留舊畫面。
fn store_captured_frame(
    buffer: &mut FrameBuffer<'_>,
    is_grayscale: bool,
    slot: &Mutex<Option<RgbaImage>>,
) {
    let Some(image) = rgba_image_from_buffer(buffer) else {
        return;
    };
    let mut image = downscale(image, MAX_CAPTURE_DIM);
    if is_grayscale {
        apply_grayscale(&mut image);
    }
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(image);
    }
}

/// 將 RGBA8 `FrameBuffer` 複製為緊密排列的 `RgbaImage`，去除每列的 `row_pitch` 填充。
fn rgba_image_from_buffer(buffer: &mut FrameBuffer<'_>) -> Option<RgbaImage> {
    let width = buffer.width();
    let height = buffer.height();
    if width == 0 || height == 0 {
        return None;
    }

    let row_pitch = buffer.row_pitch() as usize;
    let row_bytes = width as usize * 4;
    let raw = buffer.as_raw_buffer();

    // 每個位元組都會被寫入，不必先歸零。
    let mut data = Vec::with_capacity(row_bytes * height as usize);
    if row_pitch == row_bytes {
        let len = row_bytes * height as usize;
        if raw.len() < len {
            return None;
        }
        data.extend_from_slice(&raw[..len]);
    } else {
        for y in 0..height as usize {
            let src_start = y * row_pitch;
            if src_start + row_bytes > raw.len() {
                return None;
            }
            data.extend_from_slice(&raw[src_start..src_start + row_bytes]);
        }
    }

    RgbaImage::from_raw(width, height, data)
}

/// 將擷取範圍限制在螢幕內。回傳 `(x, y, width, height)`。
fn clamp_region(r: CaptureRegion, monitor_width: u32, monitor_height: u32) -> (u32, u32, u32, u32) {
    let x = r.x.min(monitor_width - 1);
    let y = r.y.min(monitor_height - 1);
    let width = r.width.min(monitor_width - x).max(1);
    let height = r.height.min(monitor_height - y).max(1);
    (x, y, width, height)
}
