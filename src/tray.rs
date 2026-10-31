//! 菜单栏托盘：图标 + 菜单 + 事件轮询。
//!
//! 事件不跨线程直接驱动 UI：App::update 每帧 try_recv 一条菜单命令，
//! 再通过 ViewportCommand 作用于窗口，全部在主线程完成，避免 Send 约束问题。

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const MENU_TOGGLE_ID: &str = "toggle_window";
const MENU_QUIT_ID: &str = "quit";

/// App 在 update 里轮询到的托盘命令
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayCommand {
    ToggleWindow,
    Quit,
}

/// 托盘句柄。TrayIcon 被 drop 时图标即从菜单栏消失，必须长期持有。
pub struct Tray {
    _icon: TrayIcon,
}

impl Tray {
    /// 创建托盘。失败（极少见）时返回 None，应用退化为普通窗口模式继续运行。
    pub fn new() -> Option<Self> {
        let menu = Menu::new();
        if let Err(err) = menu.append_items(&[
            &MenuItem::with_id(MENU_TOGGLE_ID, "显示 / 隐藏主窗口", true, None),
            &MenuItem::with_id(MENU_QUIT_ID, "退出 DisplaySmith", true, None),
        ]) {
            log::warn!("托盘菜单创建失败: {err}");
            return None;
        }

        let icon = match default_icon() {
            Ok(icon) => icon,
            Err(err) => {
                log::warn!("托盘图标生成失败: {err}");
                return None;
            }
        };

        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("DisplaySmith - 显示分辨率管理")
            .with_icon_templated(icon)
            .build()
        {
            Ok(icon) => Some(Self { _icon: icon }),
            Err(err) => {
                log::warn!("托盘创建失败（应用将以普通窗口模式运行）: {err}");
                None
            }
        }
    }

    /// 非阻塞取出一条菜单命令；无命令时返回 None
    pub fn poll_command(&self) -> Option<TrayCommand> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                MENU_TOGGLE_ID => return Some(TrayCommand::ToggleWindow),
                MENU_QUIT_ID => return Some(TrayCommand::Quit),
                _ => {}
            }
        }
        None
    }
}

/// 用代码光栅化一个 32x32 的显示器形状图标，避免引入图片资源与 image 依赖。
/// 纯黑色 + template 模式：系统自动适配菜单栏深浅色。
fn default_icon() -> Result<Icon, tray_icon::BadIcon> {
    const SIZE: usize = 32;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let outer = (4..=27).contains(&x) && (4..=23).contains(&y);
            let screen = (6..=25).contains(&x) && (6..=21).contains(&y);
            let base = (12..=19).contains(&x) && (25..=27).contains(&y);

            let pixel = if screen {
                None // 屏幕区镂空，template 图标仅保留轮廓
            } else if outer || base {
                Some([0u8, 0, 0, 255])
            } else {
                None
            };

            if let Some([r, g, b, a]) = pixel {
                let offset = (y * SIZE + x) * 4;
                rgba[offset] = r;
                rgba[offset + 1] = g;
                rgba[offset + 2] = b;
                rgba[offset + 3] = a;
            }
        }
    }

    Icon::from_rgba(rgba, SIZE as u32, SIZE as u32)
}
