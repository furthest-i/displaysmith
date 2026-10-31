//! DisplaySmith — macOS 显示器分辨率管理工具
//!
//! 入口：初始化日志，以 iced（Elm 架构）运行。
//! release 构建启动时不弹窗口，仅驻留菜单栏，面板由托盘"打开主面板"按需打开。

mod app;
mod display;
mod tray;

use iced::{window, Theme};

fn main() -> iced::Result {
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

    iced::application(
        || app::DisplaySmith::default(),
        app::update,
        app::view,
    )
    .subscription(app::subscription)
    .theme(Theme::Dark)
    .title("DisplaySmith")
    .window(window::Settings {
        size: iced::Size::new(920.0, 620.0),
        min_size: Some(iced::Size::new(740.0, 480.0)),
        // release 面向菜单栏形态：启动不弹窗，面板由托盘按需打开；debug 直接显示便于开发
        visible: cfg!(debug_assertions),
        // 关闭窗口仅收起面板，应用随托盘常驻
        exit_on_close_request: false,
        ..Default::default()
    })
    .run()
}
