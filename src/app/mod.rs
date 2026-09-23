mod background;
mod chrome;
mod drag;
mod region_picker;
mod settings_popup;

use background::Background;
use drag::DragState;
use region_picker::RegionPicker;

use crate::capture::CaptureHandle;
use crate::config::{CaptureRegion, Config};

const MIN_SIZE: f32 = 120.0;
const MAX_SIZE: f32 = 2400.0;
/// 影像相對視窗邊緣的內縮距離，讓縮放／旋轉的抓取區更容易點中。
const WINDOW_OUTER_MARGIN: f32 = 6.0;

pub struct MiniScreenApp {
    config: Config,
    capture: CaptureHandle,
    background: Background,
    settings_open: bool,
    /// 設定視窗上次量測到的尺寸；初始值刻意取寬，避免第一幀內容換行。
    settings_popup_size: egui::Vec2,
    drag_state: DragState,
    region_picker: Option<RegionPicker>,
    /// 目前實際交給擷取執行緒的範圍（見 `effective_capture_region`）。
    applied_region: Option<CaptureRegion>,
}

impl MiniScreenApp {
    pub fn new(config: Config) -> Self {
        let capture = CaptureHandle::start(config.fps, config.grayscale, config.capture_region);
        Self {
            config,
            capture,
            background: Background::new(),
            settings_open: false,
            settings_popup_size: egui::vec2(260.0, 320.0),
            drag_state: DragState::None,
            region_picker: None,
            applied_region: config.capture_region,
        }
    }

    /// 旋轉時將 `config.capture_region` 由中心擴大，使旋轉後的影像仍填滿 `inner` 顯示區，
    /// 並限制在螢幕內。全螢幕鏡像時為 `None`。
    fn effective_capture_region(&self, inner: egui::Vec2) -> Option<CaptureRegion> {
        let base = self.config.capture_region?;
        let (mon_w, mon_h) = crate::monitor::primary_monitor_dimensions()?;
        if inner.x < 1.0 || inner.y < 1.0 || base.width == 0 || base.height == 0 {
            return Some(base);
        }
        let (sin, cos) = self.config.rotation.sin_cos();
        let (sin, cos) = (sin.abs(), cos.abs());
        let need_w = inner.x * cos + inner.y * sin;
        let need_h = inner.x * sin + inner.y * cos;
        let scale = (need_w / inner.x).max(need_h / inner.y);
        let max_scale = (mon_w as f32 / base.width as f32).min(mon_h as f32 / base.height as f32);
        let scale = scale.min(max_scale).max(1.0);

        let width = ((base.width as f32 * scale).round() as u32).clamp(1, mon_w);
        let height = ((base.height as f32 * scale).round() as u32).clamp(1, mon_h);
        let center_x = base.x as f32 + base.width as f32 / 2.0;
        let center_y = base.y as f32 + base.height as f32 / 2.0;
        let x = ((center_x - width as f32 / 2.0).round().max(0.0) as u32).min(mon_w - width);
        let y = ((center_y - height as f32 / 2.0).round().max(0.0) as u32).min(mon_h - height);
        Some(CaptureRegion {
            x,
            y,
            width,
            height,
        })
    }

    /// 依擷取內容的長寬比調整視窗尺寸，避免影像被拉伸；盡量保留目前寬度。
    fn apply_capture_aspect_ratio(&mut self, ctx: &egui::Context) {
        let (content_w, content_h) = match self.config.capture_region {
            Some(r) => (r.width, r.height),
            None => match crate::monitor::primary_monitor_dimensions() {
                Some(dims) => dims,
                None => return,
            },
        };
        if content_w == 0 || content_h == 0 {
            return;
        }

        // 要符合長寬比的是扣除 WINDOW_OUTER_MARGIN 後的內部尺寸。
        let margin2 = WINDOW_OUTER_MARGIN * 2.0;
        let min_inner = MIN_SIZE - margin2;
        let max_inner = MAX_SIZE - margin2;

        let aspect = content_h as f32 / content_w as f32;
        let mut inner_width = (self.config.width - margin2).max(1.0);
        let mut inner_height = inner_width * aspect;
        if inner_height > max_inner {
            inner_height = max_inner;
            inner_width = inner_height / aspect;
        } else if inner_height < min_inner {
            inner_height = min_inner;
            inner_width = inner_height / aspect;
        }
        let inner_width = inner_width.clamp(min_inner, max_inner);
        let inner_height = inner_height.clamp(min_inner, max_inner);
        let new_size = egui::vec2(inner_width + margin2, inner_height + margin2);

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(new_size));
        self.config.width = new_size.x;
        self.config.height = new_size.y;
        self.config.save();
    }
}

/// 載入系統 CJK 字型作為後備，讓中文不會顯示成豆腐方塊。
pub fn install_cjk_font(ctx: &egui::Context) {
    const CANDIDATES: [&str; 4] = [
        r"C:\Windows\Fonts\msjh.ttc",
        r"C:\Windows\Fonts\mingliu.ttc",
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simsun.ttc",
    ];

    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            ctx.add_font(egui::epaint::text::FontInsert::new(
                "system_cjk",
                egui::FontData::from_owned(bytes),
                vec![egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Proportional,
                    priority: egui::epaint::text::FontPriority::Lowest,
                }],
            ));
            return;
        }
    }
}

/// 將主視窗排除在螢幕擷取之外（`WDA_EXCLUDEFROMCAPTURE`，Windows 10 2004+），
/// 否則擷取到的背景會包含視窗自己，而不是後方的桌面。
#[cfg(target_os = "windows")]
pub fn exclude_main_window_from_capture(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };

    let Some(window) = cc.winit_window() else {
        return;
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as windows_sys::Win32::Foundation::HWND;

    unsafe {
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn exclude_main_window_from_capture(_cc: &eframe::CreationContext<'_>) {}

impl eframe::App for MiniScreenApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 最小化時暫停擷取。
        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        self.capture.set_paused(minimized);

        // 讓背景擷取範圍跟隨視窗目前的位置與尺寸。
        let footprint = crate::monitor::window_footprint(
            self.config.x,
            self.config.y,
            self.config.width,
            self.config.height,
            ctx.pixels_per_point(),
        );
        self.capture.set_background_region(footprint);

        let rect = ui.max_rect();
        let effective_region =
            self.effective_capture_region(rect.shrink(WINDOW_OUTER_MARGIN).size());
        if effective_region != self.applied_region {
            self.applied_region = effective_region;
            self.capture.set_region(effective_region);
        }

        // 移動／縮放時不上傳紋理，否則會與系統的視窗拖曳競爭而造成延遲。
        // 旋轉不算在內，因為擷取範圍必須即時跟隨角度。
        let is_dragging = matches!(
            self.drag_state,
            DragState::Moving | DragState::Resizing { .. }
        );
        if !is_dragging {
            if let Some(image) = self.capture.take_latest_frame() {
                self.background
                    .update_content(&ctx, &image, self.applied_region);
            }
            if let Some(image) = self.capture.take_latest_background_frame() {
                self.background.update_backdrop(&ctx, &image);
            }
        }

        self.background.draw(ui, rect, &self.config);

        let button_rects = chrome::button_rects(rect);
        self.draw_buttons(ui, &ctx, &button_rects);
        self.draw_settings_popup(&ctx, &button_rects);
        self.handle_move_and_resize(&ctx, frame, rect, &button_rects);

        if self.region_picker.is_some() {
            self.draw_region_picker(&ctx);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn on_exit(&mut self) {
        self.config.save();
    }
}
