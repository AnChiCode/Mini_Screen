//! 右上角按鈕列（設定、最小化、關閉、重設旋轉）。

use super::MiniScreenApp;

const BUTTON_SIZE: f32 = 22.0;
const BUTTON_GAP: f32 = 4.0;
const BUTTON_MARGIN: f32 = 6.0;

/// 在 `rect` 的右上角由右至左排列按鈕列。
pub(crate) fn button_rects(rect: egui::Rect) -> [egui::Rect; 4] {
    let top = rect.top() + BUTTON_MARGIN;
    let mut right = rect.right() - BUTTON_MARGIN;
    let mut next = || {
        let r = egui::Rect::from_min_size(
            egui::pos2(right - BUTTON_SIZE, top),
            egui::vec2(BUTTON_SIZE, BUTTON_SIZE),
        );
        right -= BUTTON_SIZE + BUTTON_GAP;
        r
    };
    let close = next();
    let minimize = next();
    let settings = next();
    let reset_rotation = next();
    [settings, minimize, close, reset_rotation]
}

fn chrome_button() -> egui::Button<'static> {
    egui::Button::new("")
        .fill(egui::Color32::from_black_alpha(140))
        .corner_radius(egui::CornerRadius::same(4))
}

/// 圖示以手繪線條繪製，不依賴字型字符，小尺寸下也清晰。
fn icon_stroke() -> egui::Stroke {
    egui::Stroke::new(1.6, egui::Color32::WHITE)
}

fn draw_close_icon(painter: &egui::Painter, rect: egui::Rect) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.28;
    let stroke = icon_stroke();
    painter.line_segment([c + egui::vec2(-r, -r), c + egui::vec2(r, r)], stroke);
    painter.line_segment([c + egui::vec2(-r, r), c + egui::vec2(r, -r)], stroke);
}

fn draw_minimize_icon(painter: &egui::Painter, rect: egui::Rect) {
    let c = rect.center();
    let half_w = rect.width() * 0.26;
    painter.line_segment(
        [c + egui::vec2(-half_w, 0.0), c + egui::vec2(half_w, 0.0)],
        icon_stroke(),
    );
}

fn draw_settings_icon(painter: &egui::Painter, rect: egui::Rect) {
    let c = rect.center();
    let outer_r = rect.width().min(rect.height()) * 0.24;
    let inner_r = outer_r * 0.55;
    let tooth_len = outer_r * 0.45;
    let stroke = icon_stroke();

    const TEETH: usize = 8;
    for i in 0..TEETH {
        let angle = i as f32 * std::f32::consts::TAU / TEETH as f32;
        let dir = egui::vec2(angle.cos(), angle.sin());
        painter.line_segment([c + dir * outer_r, c + dir * (outer_r + tooth_len)], stroke);
    }

    painter.circle_stroke(c, outer_r, stroke);
    painter.circle_filled(c, inner_r, egui::Color32::from_black_alpha(140));
    painter.circle_stroke(c, inner_r, stroke);
}

/// 環形箭頭的「重設旋轉」圖示；未旋轉時變暗。
fn draw_reset_rotation_icon(painter: &egui::Painter, rect: egui::Rect, enabled: bool) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.26;
    let color = if enabled {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_white_alpha(70)
    };
    let stroke = egui::Stroke::new(1.6, color);

    let start_angle = -std::f32::consts::FRAC_PI_2 - 0.4;
    let sweep = std::f32::consts::TAU * 0.72;
    const SEGMENTS: usize = 14;
    let points: Vec<egui::Pos2> = (0..=SEGMENTS)
        .map(|i| {
            let t = start_angle + sweep * (i as f32 / SEGMENTS as f32);
            c + egui::vec2(t.cos(), t.sin()) * r
        })
        .collect();
    // 在 points 被移入 painter.line 前先取出最後一段，用來畫箭頭。
    let last_segment = match points.as_slice() {
        [.., prev, end] => Some((*prev, *end)),
        _ => None,
    };
    painter.line(points, stroke);

    if let Some((prev, end)) = last_segment {
        let dir = (end - prev).normalized();
        let normal = egui::vec2(-dir.y, dir.x);
        let arrow_size = r * 0.55;
        let tip = end + dir * arrow_size * 0.5;
        let left = end - normal * arrow_size * 0.5;
        let right = end + normal * arrow_size * 0.5;
        painter.add(egui::Shape::convex_polygon(
            vec![tip, left, right],
            color,
            egui::Stroke::NONE,
        ));
    }
}

impl MiniScreenApp {
    pub(crate) fn draw_buttons(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        rects: &[egui::Rect; 4],
    ) {
        let [
            settings_rect,
            minimize_rect,
            close_rect,
            reset_rotation_rect,
        ] = *rects;

        let settings_resp = ui.put(settings_rect, chrome_button());
        draw_settings_icon(ui.painter(), settings_resp.rect);
        if settings_resp.clicked() {
            self.settings_open = !self.settings_open;
        }

        let minimize_resp = ui.put(minimize_rect, chrome_button());
        draw_minimize_icon(ui.painter(), minimize_resp.rect);
        if minimize_resp.clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        let close_resp = ui.put(close_rect, chrome_button());
        draw_close_icon(ui.painter(), close_resp.rect);
        if close_resp.clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let has_rotation = self.config.rotation != 0.0;
        let reset_rotation_resp = ui.put(reset_rotation_rect, chrome_button());
        draw_reset_rotation_icon(ui.painter(), reset_rotation_resp.rect, has_rotation);
        if has_rotation && reset_rotation_resp.clicked() {
            self.config.rotation = 0.0;
            self.config.save();
        }
    }
}
