//! 鏡像畫面與視窗背景的紋理管理及繪製。

use image::RgbaImage;

use crate::config::{CaptureRegion, Config};

use super::WINDOW_OUTER_MARGIN;

const FROSTED_GLASS_FILL: egui::Color32 =
    egui::Color32::from_rgba_unmultiplied_const(230, 230, 238, 90);

pub(crate) struct Background {
    texture: Option<egui::TextureHandle>,
    /// `texture` 的實際擷取範圍；旋轉時會大於 `Config::capture_region`。
    texture_region: Option<CaptureRegion>,
    /// 視窗後方桌面的擷取畫面，用來模擬透明背景。
    background_texture: Option<egui::TextureHandle>,
}

impl Background {
    pub(crate) fn new() -> Self {
        Self {
            texture: None,
            texture_region: None,
            background_texture: None,
        }
    }

    pub(crate) fn update_content(
        &mut self,
        ctx: &egui::Context,
        image: &RgbaImage,
        region: Option<CaptureRegion>,
    ) {
        self.texture_region = region;
        set_texture(&mut self.texture, ctx, image, "mini_screen_capture");
    }

    pub(crate) fn update_backdrop(&mut self, ctx: &egui::Context, image: &RgbaImage) {
        set_texture(&mut self.background_texture, ctx, image, "mini_screen_bg");
    }

    pub(crate) fn draw(&self, ui: &mut egui::Ui, rect: egui::Rect, config: &Config) {
        let painter = ui.painter();

        // 底層背景，顯示於外側邊距與旋轉後露出的角落。wgpu 的 swapchain 不支援真正透明，
        // 因此改畫視窗後方的桌面擷取；尚無擷取時用毛玻璃色填充。
        if let Some(bg) = &self.background_texture {
            painter.image(
                bg.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            painter.rect_filled(rect, 0.0, FROSTED_GLASS_FILL);
        }

        let content_rect = rect.shrink(WINDOW_OUTER_MARGIN);
        if let Some(texture) = &self.texture {
            // 透過 UV 翻轉，不動像素資料。
            let (u0, u1) = if config.flip_x {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            };
            let (v0, v1) = if config.flip_y {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            };
            // 一般情況下填滿 `content_rect`；擷取範圍因旋轉而擴大時，依比例放大並補上偏移。
            let (size, mut offset) = match (config.capture_region, self.texture_region) {
                (Some(base), Some(grown)) if base.width > 0 && base.height > 0 => {
                    let kx = content_rect.width() / base.width as f32;
                    let ky = content_rect.height() / base.height as f32;
                    let dx = (grown.x as f32 + grown.width as f32 / 2.0)
                        - (base.x as f32 + base.width as f32 / 2.0);
                    let dy = (grown.y as f32 + grown.height as f32 / 2.0)
                        - (base.y as f32 + base.height as f32 / 2.0);
                    (
                        egui::vec2(grown.width as f32 * kx, grown.height as f32 * ky),
                        egui::vec2(dx * kx, dy * ky),
                    )
                }
                _ => (content_rect.size(), egui::Vec2::ZERO),
            };
            // 偏移量也要跟著翻轉。
            if config.flip_x {
                offset.x = -offset.x;
            }
            if config.flip_y {
                offset.y = -offset.y;
            }
            // 裁切至顯示區域，避免旋轉後溢出到外側邊距。
            draw_rotated_texture(
                &painter.with_clip_rect(content_rect),
                texture.id(),
                content_rect.center(),
                offset,
                size,
                egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1)),
                config.rotation,
            );
        }

        // 視窗邊框：深色外框加白線，在淺色與深色背景上都看得見。
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(3.0, egui::Color32::from_black_alpha(140)),
            egui::StrokeKind::Inside,
        );
        // 內縮 1px，避免緊貼裁切邊界時因像素取整而消失。
        painter.rect_stroke(
            rect.shrink(1.0),
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(230)),
            egui::StrokeKind::Inside,
        );
    }
}

/// 將 `image` 上傳到 `slot`，首次建立紋理、之後原地更新。
fn set_texture(
    slot: &mut Option<egui::TextureHandle>,
    ctx: &egui::Context,
    image: &RgbaImage,
    name: &str,
) {
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    match slot {
        Some(texture) => texture.set(color_image, egui::TextureOptions::LINEAR),
        None => {
            *slot = Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR));
        }
    }
}

/// 將紋理畫成尺寸 `size`、中心位於 `pivot + offset` 的四邊形，再繞 `pivot` 旋轉 `rotation` 弧度。
fn draw_rotated_texture(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    pivot: egui::Pos2,
    offset: egui::Vec2,
    size: egui::Vec2,
    uv: egui::Rect,
    rotation: f32,
) {
    let half = size / 2.0;
    let (sin, cos) = rotation.sin_cos();
    let rotate = |dx: f32, dy: f32| {
        let (x, y) = (offset.x + dx, offset.y + dy);
        egui::pos2(pivot.x + x * cos - y * sin, pivot.y + x * sin + y * cos)
    };

    let positions = [
        rotate(-half.x, -half.y),
        rotate(half.x, -half.y),
        rotate(half.x, half.y),
        rotate(-half.x, half.y),
    ];
    let uvs = [
        uv.min,
        egui::pos2(uv.max.x, uv.min.y),
        uv.max,
        egui::pos2(uv.min.x, uv.max.y),
    ];

    let mut mesh = egui::Mesh::with_texture(texture_id);
    for i in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: positions[i],
            uv: uvs[i],
            color: egui::Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 2, 3, 0]);
    painter.add(egui::Shape::mesh(mesh));
}
