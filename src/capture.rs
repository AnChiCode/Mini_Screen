//! 各平台的螢幕擷取，對外統一為 `CaptureHandle` API。
//!
//! - Windows（`capture::windows`）：持續運作的 `windows-capture` 工作階段，
//!   另外擷取視窗自身後方的桌面作為背景。
//! - Linux（`capture::linux`）：輪詢 `xcap`。不支援背景擷取，見該模組說明。

use image::RgbaImage;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::CaptureHandle;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::CaptureHandle;

/// 畫面交給 UI 前縮小到的最長邊；視窗只顯示縮圖，上傳原生解析度只是浪費頻寬。
pub(crate) const MAX_CAPTURE_DIM: u32 = 1280;

/// 直接索引緩衝區的最近鄰縮小。
///
/// 不用 `image::imageops::resize`：其泛型逐像素抽象即使在 `Nearest` 下，
/// 一張 4K 畫面也要數十毫秒，會拖垮 FPS。
pub(crate) fn downscale(image: RgbaImage, max_dim: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_dim {
        return image;
    }
    let scale = max_dim as f32 / longest as f32;
    let new_width = ((width as f32) * scale).round().max(1.0) as u32;
    let new_height = ((height as f32) * scale).round().max(1.0) as u32;

    let src = image.as_raw();
    // dst 會被完整寫入，不必先歸零。
    let mut dst = Vec::with_capacity((new_width * new_height * 4) as usize);

    for dy in 0..new_height {
        let sy = dy * height / new_height;
        let src_row = (sy * width * 4) as usize;
        for dx in 0..new_width {
            let sx = (dx * width / new_width * 4) as usize;
            dst.extend_from_slice(&src[src_row + sx..src_row + sx + 4]);
        }
    }

    RgbaImage::from_raw(new_width, new_height, dst)
        .expect("downscale buffer size matches dimensions")
}

pub(crate) fn apply_grayscale(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let luma = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
        pixel.0 = [luma, luma, luma, a];
    }
}
