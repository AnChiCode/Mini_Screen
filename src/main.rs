// Windows 上以 GUI 子系統連結，避免啟動時多開一個主控台視窗。
#![windows_subsystem = "windows"]

mod app;
mod capture;
mod config;
mod monitor;

use app::MiniScreenApp;
use config::Config;

/// 視窗／工作列圖示。
fn load_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("assets/icon.png is a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    let config = Config::load();

    let viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(true)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_position([config.x, config.y])
        .with_inner_size([config.width, config.height])
        .with_icon(load_icon());

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "mini_screen",
        native_options,
        Box::new(|cc| {
            app::install_cjk_font(&cc.egui_ctx);
            app::exclude_main_window_from_capture(cc);
            Ok(Box::new(MiniScreenApp::new(config)))
        }),
    )
}
