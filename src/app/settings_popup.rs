//! 設定面板。
//!
//! - Windows：獨立的系統視窗，不會被主視窗的範圍裁切。
//! - Linux：主視窗內的 `egui::Window`，因為 Wayland 無法定位第二個視窗。
//!
//! 兩者共用 `draw_settings_content` 與 `apply_settings_changes`。

use crate::config::{self, CaptureRegion};

use super::MiniScreenApp;

/// 設定視窗與螢幕邊緣的最小間距。
#[cfg(target_os = "windows")]
const SCREEN_EDGE_MARGIN: f32 = 12.0;

/// 設定表單的編輯狀態，繪製後由 `apply_settings_changes` 比對並套用變更。
struct SettingsFormState {
    fps: u32,
    flip_x: bool,
    flip_y: bool,
    /// 本幀開始時正規化後的角度，用來偵測使用者是否拖動滑桿；
    /// `Config::rotation` 可能超過一圈，不能直接比較。
    initial_rotation_deg: f32,
    rotation_deg: f32,
    grayscale: bool,
    start_region_picker: bool,
    reset_to_fullscreen: bool,
}

impl SettingsFormState {
    fn from_config(config: &config::Config) -> Self {
        let initial_rotation_deg = normalize_degrees(config.rotation.to_degrees());
        Self {
            fps: config.fps,
            flip_x: config.flip_x,
            flip_y: config.flip_y,
            initial_rotation_deg,
            rotation_deg: initial_rotation_deg,
            grayscale: config.grayscale,
            start_region_picker: false,
            reset_to_fullscreen: false,
        }
    }
}

/// 繪製設定元件（兩個平台共用）。
fn draw_settings_content(
    ui: &mut egui::Ui,
    capture_region: Option<CaptureRegion>,
    state: &mut SettingsFormState,
) {
    ui.label("每秒幀率 (FPS)");
    ui.add(egui::Slider::new(
        &mut state.fps,
        config::MIN_FPS..=config::MAX_FPS,
    ));

    ui.separator();
    ui.label("畫面反轉");
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.flip_x, "X 軸");
        ui.checkbox(&mut state.flip_y, "Y 軸");
    });

    ui.separator();
    ui.label("旋轉角度");
    ui.add(egui::Slider::new(&mut state.rotation_deg, -180.0..=180.0).suffix("°"));

    ui.separator();
    ui.checkbox(&mut state.grayscale, "灰階濾鏡");

    ui.separator();
    ui.label("擷取範圍");
    let region_text = match capture_region {
        Some(r) => format!("目前：{}×{} @ ({}, {})", r.width, r.height, r.x, r.y),
        None => "目前：全螢幕".to_owned(),
    };
    ui.label(region_text);
    ui.horizontal(|ui| {
        if ui.button("設定擷取範圍").clicked() {
            state.start_region_picker = true;
        }
        if ui
            .add_enabled(capture_region.is_some(), egui::Button::new("全螢幕"))
            .clicked()
        {
            state.reset_to_fullscreen = true;
        }
    });
}

impl MiniScreenApp {
    /// 將表單中有變更的欄位套用到 `config`/`capture` 並儲存。
    fn apply_settings_changes(&mut self, ctx: &egui::Context, state: &SettingsFormState) {
        if state.fps != self.config.fps {
            self.config.fps = state.fps;
            self.capture.set_fps(state.fps);
            self.config.save();
        }
        if state.flip_x != self.config.flip_x || state.flip_y != self.config.flip_y {
            self.config.flip_x = state.flip_x;
            self.config.flip_y = state.flip_y;
            self.config.save();
        }
        if state.rotation_deg != state.initial_rotation_deg {
            self.config.rotation = state.rotation_deg.to_radians();
            self.config.save();
        }
        if state.grayscale != self.config.grayscale {
            self.config.grayscale = state.grayscale;
            self.capture.set_grayscale(state.grayscale);
            self.config.save();
        }
        if state.reset_to_fullscreen {
            self.config.capture_region = None;
            self.apply_capture_aspect_ratio(ctx);
        }
        if state.start_region_picker {
            self.begin_region_picker(ctx);
        }
    }
}

#[cfg(target_os = "windows")]
impl MiniScreenApp {
    pub(crate) fn draw_settings_popup(
        &mut self,
        ctx: &egui::Context,
        button_rects: &[egui::Rect; 4],
    ) {
        use crate::monitor;

        if !self.settings_open {
            return;
        }
        let settings_rect = button_rects[0];
        let root_outer_min = ctx
            .input(|i| i.viewport().outer_rect)
            .map(|r| r.min)
            .unwrap_or(egui::Pos2::ZERO);
        let popup_size = self.settings_popup_size;
        let popup_pos = monitor::clamp_to_screen(
            root_outer_min + egui::vec2(settings_rect.left(), settings_rect.bottom() + 6.0),
            popup_size,
            SCREEN_EDGE_MARGIN,
            ctx.pixels_per_point(),
        );

        let mut state = SettingsFormState::from_config(&self.config);
        let capture_region = self.config.capture_region;
        let mut still_open = true;
        let mut measured_size = popup_size;

        let viewport_id = egui::ViewportId::from_hash_of("mini_screen_settings_popup");
        let builder = egui::ViewportBuilder::default()
            .with_title("設定")
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop)
            .with_position(popup_pos)
            .with_inner_size(popup_size);

        ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
            let content = egui::Frame::popup(&ui.ctx().style_of(ui.ctx().theme())).show(ui, |ui| {
                draw_settings_content(ui, capture_region, &mut state);
            });
            measured_size = content.response.rect.size();

            let (focused, escape, close_requested) = ui.ctx().input(|i| {
                (
                    i.viewport().focused,
                    i.key_pressed(egui::Key::Escape),
                    i.viewport().close_requested(),
                )
            });
            if focused == Some(false) || escape || close_requested || state.start_region_picker {
                still_open = false;
            }
        });

        if (measured_size - self.settings_popup_size).length_sq() > 0.25 {
            self.settings_popup_size = measured_size;
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::InnerSize(measured_size));

            let new_pos = monitor::clamp_to_screen(
                root_outer_min + egui::vec2(settings_rect.left(), settings_rect.bottom() + 6.0),
                measured_size,
                SCREEN_EDGE_MARGIN,
                ctx.pixels_per_point(),
            );
            if new_pos != popup_pos {
                ctx.send_viewport_cmd_to(
                    viewport_id,
                    egui::ViewportCommand::OuterPosition(new_pos),
                );
            }
        }

        if !still_open {
            self.settings_open = false;
        }
        self.apply_settings_changes(ctx, &state);
    }
}

#[cfg(target_os = "linux")]
impl MiniScreenApp {
    pub(crate) fn draw_settings_popup(
        &mut self,
        ctx: &egui::Context,
        button_rects: &[egui::Rect; 4],
    ) {
        if !self.settings_open {
            return;
        }
        let settings_rect = button_rects[0];
        // 主視窗內的座標；`egui::Window` 會自動限制在主視窗範圍內。
        let popup_pos = egui::pos2(settings_rect.left(), settings_rect.bottom() + 6.0);

        let mut state = SettingsFormState::from_config(&self.config);
        let capture_region = self.config.capture_region;
        let mut still_open = true;

        egui::Window::new("設定")
            .id(egui::Id::new("mini_screen_settings_popup"))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .current_pos(popup_pos)
            .frame(egui::Frame::popup(&ctx.style_of(ctx.theme())))
            .show(ctx, |ui| {
                draw_settings_content(ui, capture_region, &mut state);
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || state.start_region_picker {
            still_open = false;
        }

        if !still_open {
            self.settings_open = false;
        }
        self.apply_settings_changes(ctx, &state);
    }
}

/// 將角度正規化到 `(-180, 180]`。
fn normalize_degrees(deg: f32) -> f32 {
    let wrapped = deg.rem_euclid(360.0);
    if wrapped > 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}
