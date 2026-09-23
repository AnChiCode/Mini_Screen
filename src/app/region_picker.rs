//! 全螢幕覆蓋層，讓使用者拖曳選擇擷取範圍。按 Esc 或未拖曳的點擊會取消。

use crate::capture::CaptureHandle;
use crate::config::CaptureRegion;
use crate::monitor;

use super::MiniScreenApp;

pub(crate) struct RegionPicker {
    /// 螢幕位置／尺寸（egui 邏輯點）。
    monitor_pos: egui::Pos2,
    monitor_size: egui::Vec2,
    /// 用來把選取範圍轉回 `CaptureRegion` 的實體像素。
    pixels_per_point: f32,
    drag_start: Option<egui::Pos2>,
    texture: Option<egui::TextureHandle>,
}

impl RegionPicker {
    fn begin(ctx: &egui::Context, capture: &CaptureHandle) -> Option<Self> {
        let (x, y, width, height) = monitor::primary_monitor_rect_physical()?;
        let pixels_per_point = ctx.pixels_per_point();

        // 以完整螢幕快照當背景，不依賴視窗透明。
        let texture = capture.snapshot().map(|image| {
            let size = [image.width() as usize, image.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            ctx.load_texture(
                "mini_screen_region_picker_bg",
                color_image,
                egui::TextureOptions::LINEAR,
            )
        });

        Some(Self {
            monitor_pos: egui::pos2(x as f32 / pixels_per_point, y as f32 / pixels_per_point),
            monitor_size: egui::vec2(
                width as f32 / pixels_per_point,
                height as f32 / pixels_per_point,
            ),
            pixels_per_point,
            drag_start: None,
            texture,
        })
    }
}

impl MiniScreenApp {
    pub(crate) fn begin_region_picker(&mut self, ctx: &egui::Context) {
        self.region_picker = RegionPicker::begin(ctx, &self.capture);
    }

    pub(crate) fn draw_region_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = &self.region_picker else {
            return;
        };
        let monitor_pos = picker.monitor_pos;
        let monitor_size = picker.monitor_size;
        let pixels_per_point = picker.pixels_per_point;
        let mut drag_start = picker.drag_start;
        let background_texture = picker.texture.as_ref().map(|t| t.id());

        let mut still_active = true;
        let mut finalized_region: Option<CaptureRegion> = None;

        let viewport_id = egui::ViewportId::from_hash_of("mini_screen_region_picker");
        let builder = egui::ViewportBuilder::default()
            .with_title("選擇擷取範圍")
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        // Wayland 會忽略 `with_position`，因此 Linux 上改用全螢幕覆蓋。
        #[cfg(target_os = "linux")]
        let builder = builder.with_fullscreen(true);
        #[cfg(not(target_os = "linux"))]
        let builder = builder
            .with_position(monitor_pos)
            .with_inner_size(monitor_size);

        ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
            let rect = ui.max_rect();
            let (hover_pos, primary_pressed, any_released, escape) = ui.ctx().input(|i| {
                (
                    i.pointer.hover_pos(),
                    i.pointer.primary_pressed(),
                    i.pointer.any_released(),
                    i.key_pressed(egui::Key::Escape),
                )
            });

            if escape {
                still_active = false;
                return;
            }

            if primary_pressed && drag_start.is_none() {
                drag_start = hover_pos;
            }

            let current_rect = match (drag_start, hover_pos) {
                (Some(start), Some(current)) => {
                    Some(egui::Rect::from_two_pos(start, current).intersect(rect))
                }
                _ => None,
            };

            let painter = ui.painter();
            if let Some(texture_id) = background_texture {
                painter.image(
                    texture_id,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            let dim = egui::Color32::from_black_alpha(120);
            if let Some(sel) = current_rect {
                painter.rect_filled(
                    egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, sel.min.y)),
                    0.0,
                    dim,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(rect.min.x, sel.max.y), rect.max),
                    0.0,
                    dim,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, sel.min.y),
                        egui::pos2(sel.min.x, sel.max.y),
                    ),
                    0.0,
                    dim,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(sel.max.x, sel.min.y),
                        egui::pos2(rect.max.x, sel.max.y),
                    ),
                    0.0,
                    dim,
                );
                painter.rect_stroke(
                    sel,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 200, 0)),
                    egui::StrokeKind::Outside,
                );
                painter.text(
                    sel.left_top() + egui::vec2(4.0, -20.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{:.0} x {:.0}", sel.width(), sel.height()),
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                );
            } else {
                painter.rect_filled(rect, 0.0, dim);
            }

            painter.text(
                rect.center_top() + egui::vec2(0.0, 12.0),
                egui::Align2::CENTER_TOP,
                "拖曳以選擇擷取範圍，按 Esc 取消",
                egui::FontId::proportional(16.0),
                egui::Color32::WHITE,
            );

            if any_released {
                if let Some(sel) = current_rect
                    && sel.width() >= 8.0
                    && sel.height() >= 8.0
                {
                    // 邏輯點轉回實體像素。
                    finalized_region = Some(CaptureRegion {
                        x: ((sel.min.x - monitor_pos.x) * pixels_per_point)
                            .max(0.0)
                            .round() as u32,
                        y: ((sel.min.y - monitor_pos.y) * pixels_per_point)
                            .max(0.0)
                            .round() as u32,
                        width: (sel.width() * pixels_per_point).round().max(1.0) as u32,
                        height: (sel.height() * pixels_per_point).round().max(1.0) as u32,
                    });
                }
                still_active = false;
            }

            if ui.ctx().input(|i| i.viewport().close_requested()) {
                still_active = false;
            }
        });

        if let Some(p) = &mut self.region_picker {
            p.drag_start = drag_start;
        }
        if !still_active {
            self.region_picker = None;
        }
        if let Some(region) = finalized_region {
            self.config.capture_region = Some(region);
            self.apply_capture_aspect_ratio(ctx);
        }
    }
}
