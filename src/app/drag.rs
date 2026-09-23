//! 視窗的拖曳、縮放與旋轉互動。

use super::{MAX_SIZE, MIN_SIZE, MiniScreenApp};

const RESIZE_MARGIN: f32 = 8.0;
/// 角落（旋轉）命中區比邊緣（縮放）更大，較容易點中。
const ROTATE_MARGIN: f32 = RESIZE_MARGIN + 6.0;

#[derive(Clone, Copy)]
pub(crate) enum DragState {
    None,
    Moving,
    Resizing {
        handle: Handle,
        /// 拖曳開始時的指標位置（螢幕座標，見 `cursor_pos_screen`）。
        start_pointer_screen: egui::Pos2,
        start_outer_pos: egui::Pos2,
        start_size: egui::Vec2,
    },
    Rotating {
        start_pointer: egui::Pos2,
        start_rotation: f32,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    /// 邊緣縮放、角落旋轉，所以角落用十字游標。
    fn cursor_icon(self) -> egui::CursorIcon {
        match self {
            Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
            Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
            Handle::NE | Handle::NW | Handle::SE | Handle::SW => egui::CursorIcon::Crosshair,
        }
    }

    fn is_corner(self) -> bool {
        matches!(self, Handle::NE | Handle::NW | Handle::SE | Handle::SW)
    }

    fn has_west(self) -> bool {
        matches!(self, Handle::W | Handle::NW | Handle::SW)
    }

    fn has_east(self) -> bool {
        matches!(self, Handle::E | Handle::NE | Handle::SE)
    }

    fn has_north(self) -> bool {
        matches!(self, Handle::N | Handle::NW | Handle::NE)
    }

    fn has_south(self) -> bool {
        matches!(self, Handle::S | Handle::SW | Handle::SE)
    }
}

fn handle_at(pos: egui::Pos2, size: egui::Vec2) -> Option<Handle> {
    if pos.x < 0.0 || pos.y < 0.0 || pos.x > size.x || pos.y > size.y {
        return None;
    }

    // 角落優先於邊緣判定。
    let west_c = pos.x <= ROTATE_MARGIN;
    let east_c = pos.x >= size.x - ROTATE_MARGIN;
    let north_c = pos.y <= ROTATE_MARGIN;
    let south_c = pos.y >= size.y - ROTATE_MARGIN;
    match (west_c, east_c, north_c, south_c) {
        (true, _, true, _) => return Some(Handle::NW),
        (_, true, true, _) => return Some(Handle::NE),
        (true, _, _, true) => return Some(Handle::SW),
        (_, true, _, true) => return Some(Handle::SE),
        _ => {}
    }

    let west = pos.x <= RESIZE_MARGIN;
    let east = pos.x >= size.x - RESIZE_MARGIN;
    let north = pos.y <= RESIZE_MARGIN;
    let south = pos.y >= size.y - RESIZE_MARGIN;
    match (west, east, north, south) {
        (true, false, false, false) => resize_handle(Handle::W),
        (false, true, false, false) => Some(Handle::E),
        (false, false, true, false) => resize_handle(Handle::N),
        (false, false, false, true) => Some(Handle::S),
        _ => None,
    }
}

/// 從 W/N 邊縮放需要移動視窗位置，而 Wayland 無法取得或設定絕對位置，因此 Linux 上停用這兩邊。
#[cfg(target_os = "linux")]
fn resize_handle(_handle: Handle) -> Option<Handle> {
    None
}

#[cfg(not(target_os = "linux"))]
fn resize_handle(handle: Handle) -> Option<Handle> {
    Some(handle)
}

/// 計算等比例縮放後的視窗尺寸與位置，固定被拖曳邊的對側。指標位置須為螢幕座標。
fn compute_resize(
    handle: Handle,
    start_pointer_screen: egui::Pos2,
    start_outer_pos: egui::Pos2,
    start_size: egui::Vec2,
    current_pointer_screen: egui::Pos2,
) -> (egui::Vec2, egui::Pos2) {
    let dx = current_pointer_screen.x - start_pointer_screen.x;
    let dy = current_pointer_screen.y - start_pointer_screen.y;

    let growth_x = if handle.has_west() { -dx } else { dx };
    let growth_y = if handle.has_north() { -dy } else { dy };

    let mut ratio = 0.0;
    let mut axes = 0;
    if handle.has_east() || handle.has_west() {
        ratio += growth_x / start_size.x;
        axes += 1;
    }
    if handle.has_north() || handle.has_south() {
        ratio += growth_y / start_size.y;
        axes += 1;
    }
    if axes > 0 {
        ratio /= axes as f32;
    }

    let min_scale = (MIN_SIZE / start_size.x).max(MIN_SIZE / start_size.y);
    let max_scale = (MAX_SIZE / start_size.x).min(MAX_SIZE / start_size.y);
    // 若系統把視窗調整到範圍外，可能出現 min > max，而 `f32::clamp` 會因此 panic。
    let (min_scale, max_scale) = (min_scale.min(max_scale), min_scale.max(max_scale));
    let scale = (1.0 + ratio).clamp(min_scale, max_scale);

    let new_size = start_size * scale;

    let anchor_right = start_outer_pos.x + start_size.x;
    let anchor_bottom = start_outer_pos.y + start_size.y;

    let new_x = if handle.has_west() {
        anchor_right - new_size.x
    } else {
        start_outer_pos.x
    };
    let new_y = if handle.has_north() {
        anchor_bottom - new_size.y
    } else {
        start_outer_pos.y
    };

    // 防止 NaN 等無效值傳給視窗，否則 wgpu 可能 panic。
    let new_size = if new_size.x.is_finite() && new_size.y.is_finite() {
        egui::vec2(
            new_size.x.clamp(MIN_SIZE, MAX_SIZE),
            new_size.y.clamp(MIN_SIZE, MAX_SIZE),
        )
    } else {
        start_size
    };
    let new_pos = if new_x.is_finite() && new_y.is_finite() {
        egui::pos2(new_x, new_y)
    } else {
        start_outer_pos
    };

    (new_size, new_pos)
}

/// 直接向系統讀取游標的螢幕座標（邏輯點）。縮放時視窗由 `SetWindowPos` 移動，
/// `outer_rect` 可能過時，不能用來推算。
#[cfg(target_os = "windows")]
fn cursor_pos_screen(pixels_per_point: f32) -> Option<egui::Pos2> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    Some(egui::pos2(
        point.x as f32 / pixels_per_point,
        point.y as f32 / pixels_per_point,
    ))
}

#[cfg(not(target_os = "windows"))]
fn cursor_pos_screen(_pixels_per_point: f32) -> Option<egui::Pos2> {
    None
}

/// 一次設定視窗位置與尺寸。Windows 上用單一 `SetWindowPos`，避免分兩步套用造成閃爍。
fn apply_window_bounds(
    ctx: &egui::Context,
    frame: &eframe::Frame,
    new_pos: egui::Pos2,
    new_size: egui::Vec2,
) {
    #[cfg(target_os = "windows")]
    if set_window_bounds_win32(frame, ctx.pixels_per_point(), new_pos, new_size) {
        return;
    }

    let _ = frame;
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(new_size));
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(new_pos));
}

#[cfg(target_os = "windows")]
fn set_window_bounds_win32(
    frame: &eframe::Frame,
    pixels_per_point: f32,
    pos: egui::Pos2,
    size: egui::Vec2,
) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let Some(window) = frame.winit_window() else {
        return false;
    };
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return false;
    };

    let hwnd = win32.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let x = (pos.x * pixels_per_point).round() as i32;
    let y = (pos.y * pixels_per_point).round() as i32;
    let cx = ((size.x * pixels_per_point).round() as i32).max(1);
    let cy = ((size.y * pixels_per_point).round() as i32).max(1);

    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            cx,
            cy,
            SWP_NOZORDER | SWP_NOACTIVATE,
        ) != 0
    }
}

impl MiniScreenApp {
    pub(crate) fn handle_move_and_resize(
        &mut self,
        ctx: &egui::Context,
        frame: &eframe::Frame,
        rect: egui::Rect,
        button_rects: &[egui::Rect; 4],
    ) {
        let size = rect.size();
        let (hover_pos, press_origin, primary_pressed, primary_down, outer_pos) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.press_origin(),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.viewport().outer_rect.map(|r| r.min),
            )
        });

        let over_ui = hover_pos
            .map(|p| button_rects.iter().any(|r| r.contains(p)))
            .unwrap_or(false);

        match self.drag_state {
            DragState::None => {
                if primary_pressed && !over_ui {
                    if let Some(press_pos) = press_origin {
                        if let Some(handle) = handle_at(press_pos, size) {
                            if handle.is_corner() {
                                self.drag_state = DragState::Rotating {
                                    start_pointer: press_pos,
                                    start_rotation: self.config.rotation,
                                };
                            } else {
                                let start_outer_pos = outer_pos.unwrap_or(rect.min);
                                let start_pointer_screen =
                                    cursor_pos_screen(ctx.pixels_per_point())
                                        .unwrap_or(press_pos + start_outer_pos.to_vec2());
                                self.drag_state = DragState::Resizing {
                                    handle,
                                    start_pointer_screen,
                                    start_outer_pos,
                                    start_size: size,
                                };
                            }
                        } else {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            self.drag_state = DragState::Moving;
                        }
                    }
                } else if !over_ui
                    && let Some(pos) = hover_pos
                    && let Some(handle) = handle_at(pos, size)
                {
                    ctx.set_cursor_icon(handle.cursor_icon());
                }
            }
            DragState::Moving => {
                if !primary_down {
                    if let Some(pos) = outer_pos {
                        self.config.x = pos.x;
                        self.config.y = pos.y;
                    }
                    self.config.save();
                    self.drag_state = DragState::None;
                }
            }
            DragState::Resizing {
                handle,
                start_pointer_screen,
                start_outer_pos,
                start_size,
            } => {
                if primary_down {
                    // 依序嘗試：系統游標位置 → hover_pos + outer_rect → 視窗區域座標
                    // （Wayland；只有 E/S 邊，不移動視窗，所以結果一樣正確）。
                    let current_screen = cursor_pos_screen(ctx.pixels_per_point())
                        .or_else(|| hover_pos.zip(outer_pos).map(|(c, o)| c + o.to_vec2()))
                        .or_else(|| hover_pos.map(|p| p + rect.min.to_vec2()));
                    if let Some(current_screen) = current_screen {
                        let (new_size, new_pos) = compute_resize(
                            handle,
                            start_pointer_screen,
                            start_outer_pos,
                            start_size,
                            current_screen,
                        );
                        apply_window_bounds(ctx, frame, new_pos, new_size);
                        self.config.width = new_size.x;
                        self.config.height = new_size.y;
                        self.config.x = new_pos.x;
                        self.config.y = new_pos.y;
                        ctx.set_cursor_icon(handle.cursor_icon());
                    }
                } else {
                    self.config.save();
                    self.drag_state = DragState::None;
                }
            }
            DragState::Rotating {
                start_pointer,
                start_rotation,
            } => {
                if primary_down {
                    if let Some(current) = hover_pos {
                        let center = rect.center();
                        let start_vec = start_pointer - center;
                        let current_vec = current - center;
                        let start_angle = start_vec.y.atan2(start_vec.x);
                        let current_angle = current_vec.y.atan2(current_vec.x);
                        self.config.rotation = start_rotation + (current_angle - start_angle);
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                } else {
                    self.config.save();
                    self.drag_state = DragState::None;
                }
            }
        }
    }
}
