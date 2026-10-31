//! DisplaySmith — macOS 显示器分辨率管理工具
//!
//! 入口：初始化日志，以 glow 后端启动 eframe 主窗口。
//! 托盘图标在 App 构建时创建，由 App 持有保活。

mod app;
mod display;
mod fonts;
mod tray;

use eframe::egui;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([920.0, 620.0])
            .with_min_inner_size([740.0, 480.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "DisplaySmith",
        options,
        Box::new(|cc| Ok(Box::new(app::DisplaySmithApp::new(cc)))),
    )
}
