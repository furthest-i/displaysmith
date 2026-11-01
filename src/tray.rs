//! 菜单栏托盘：图标 + 菜单 + 事件流。
//!
//! iced 侧通过 `Subscription::run(tray_stream)` 订阅菜单命令，
//! 命令以 Message 推入 update，全程在 iced 的运行时内流转。

use std::time::Duration;

use iced::futures::Stream;
use iced::futures::channel::mpsc::Sender;
use iced::futures::SinkExt;
use iced::stream;
use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// 托盘菜单项 id
pub const MENU_OPEN_ID: &str = "open_panel";
pub const MENU_QUIT_ID: &str = "quit";

/// 托盘菜单命令
#[derive(Debug, Clone)]
pub enum TrayCommand {
    OpenPanel,
    Quit,
}

/// 托盘句柄。TrayIcon 被 drop 时图标即从菜单栏消失，必须长期持有。
pub struct Tray {
    _icon: TrayIcon,
}

impl Tray {
    /// 创建托盘。失败（极少见）时返回 None，应用退化为无托盘模式继续运行。
    pub fn new() -> Option<Self> {
        let menu = Menu::new();
        if let Err(err) = menu.append_items(&[
            &MenuItem::with_id(MENU_OPEN_ID, "打开主面板", true, None),
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
                log::warn!("托盘创建失败（应用将以无托盘模式运行）: {err}");
                None
            }
        }
    }
}

/// 托盘菜单命令流。
///
/// crossbeam 的 `recv_timeout` 是阻塞调用，不能直接跑在 iced 的线程池订阅上——
/// 它会占死执行线程，导致同一 stream 上向 runtime 递交消息的另一半永远不被轮询
/// （表现为菜单事件收到、界面却无响应）。因此把阻塞轮询放到独立线程，
/// 经无界 channel 桥接回异步侧，转发线程在无事件时真正挂起让出。
pub fn tray_stream() -> impl Stream<Item = TrayCommand> {
    stream::channel(8, |mut output: Sender<TrayCommand>| async move {
        let (forward_tx, mut forward_rx) =
            iced::futures::channel::mpsc::unbounded::<TrayCommand>();

        std::thread::spawn(move || {
            let receiver = tray_icon::menu::MenuEvent::receiver();
            loop {
                match receiver.recv_timeout(Duration::from_millis(500)) {
                    Ok(event) => {
                        log::info!("收到托盘菜单事件: id={:?}", event.id.0);
                        let command = match event.id.0.as_str() {
                            MENU_OPEN_ID => Some(TrayCommand::OpenPanel),
                            MENU_QUIT_ID => Some(TrayCommand::Quit),
                            _ => None,
                        };
                        if let Some(command) = command
                            && forward_tx.unbounded_send(command).is_err()
                        {
                            break; // 应用退出，转发通道已关闭
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        use iced::futures::StreamExt;
        while let Some(command) = forward_rx.next().await {
            let _ = output.send(command).await;
        }
    })
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
