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

    // 自检开关：对主显示器重新应用其当前模式，完整验证配置事务链路而不产生视觉变化
    if std::env::args().any(|arg| arg == "--apply-self-test") {
        let displays = display::list_displays();
        let Some(main_display) = displays.iter().find(|d| d.is_main) else {
            eprintln!("self-test 失败: 未找到主显示器");
            std::process::exit(1);
        };
        match display::apply_mode(main_display.id, &main_display.current) {
            Ok(()) => println!(
                "self-test 通过: 主显示器事务链路正常 ({})",
                main_display.current.pixel_label()
            ),
            Err(err) => {
                eprintln!("self-test 失败: {err}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

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
