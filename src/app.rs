//! 主窗口 UI（iced Elm 架构）+ 托盘命令处理。
//!
//! 状态流转：Subscription 推送托盘命令 / 窗口关闭事件，widget 交互直接发 Message，
//! update 统一修改状态，view 由状态推导整棵界面树。

use iced::widget::space;
use iced::widget::{button, column, pick_list, row, scrollable, table, text};
use iced::{window, Color, Element, Subscription, Task, Theme};
use tray_icon::TrayIcon;

use crate::display::{self, DisplayInfo, DisplayModeInfo};
use crate::tray::{self, Tray, TrayCommand};

/// 常用色：成功/可用状态
const COLOR_OK: Color = Color::from_rgb(0.44, 0.75, 0.44);
/// 常用色：提示/主显示器标识
const COLOR_INFO: Color = Color::from_rgb(0.45, 0.65, 0.95);

/// 下拉筛选的选项值：`All` 表示不筛选
#[derive(Debug, Clone, PartialEq)]
pub struct FilterOption(pub Option<String>);

impl std::fmt::Display for FilterOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(value) => write!(f, "{value}"),
            None => write!(f, "全部"),
        }
    }
}

/// 应用状态
pub struct DisplaySmith {
    tray: Option<Tray>,
    displays: Vec<DisplayInfo>,
    /// 当前选中的显示器
    selected: Option<DisplayInfo>,
    modes: Vec<DisplayModeInfo>,
    /// 比例筛选（None = 全部），取值为 aspect_label 文本
    aspect_filter: Option<String>,
    /// 刷新率筛选（None = 全部），取值为取整后的显示文本（如 "120"）
    hz_filter: Option<String>,
    /// 底部状态栏的"最近操作"反馈，新操作覆盖旧消息
    status_message: Option<String>,
    /// 托盘打开的主面板窗口；None = 未打开（启动时的初始窗口不计入）
    panel_id: Option<window::Id>,
}

impl Default for DisplaySmith {
    fn default() -> Self {
        let displays = display::list_displays();
        let selected = main_display(&displays);
        let modes = selected
            .as_ref()
            .map(|d| display::list_modes(d.id))
            .unwrap_or_default();
        log::info!("启动完成: {} 台显示器", displays.len());

        Self {
            tray: Tray::new(),
            displays,
            selected,
            modes,
            aspect_filter: None,
            hz_filter: None,
            status_message: None,
            panel_id: None,
        }
    }
}

impl DisplaySmith {
    /// 重新枚举显示器与模式；选中显示器按 id 保持，失联则回退主显示器
    fn refresh(&mut self) {
        self.displays = display::list_displays();
        let still_online = self
            .selected
            .as_ref()
            .is_some_and(|selected| self.displays.iter().any(|d| d.id == selected.id));
        if !still_online {
            self.selected = main_display(&self.displays);
        }
        self.modes = self
            .selected
            .as_ref()
            .map(|d| display::list_modes(d.id))
            .unwrap_or_default();
        self.aspect_filter = None;
        self.hz_filter = None;
        log::info!(
            "已刷新: {} 台显示器, {} 个模式",
            self.displays.len(),
            self.modes.len()
        );
    }

    /// 切换选中的显示器并加载其模式列表
    fn select_display(&mut self, info: &DisplayInfo) {
        if self.selected.as_ref().map(|s| s.id) == Some(info.id) {
            return;
        }
        self.selected = Some(info.clone());
        self.modes = display::list_modes(info.id);
        self.aspect_filter = None;
        self.hz_filter = None;
    }

    /// 应用筛选条件后的模式列表
    fn filtered_modes(&self) -> Vec<DisplayModeInfo> {
        self.modes
            .iter()
            .filter(|mode| {
                let aspect_ok = self
                    .aspect_filter
                    .as_ref()
                    .is_none_or(|f| mode.aspect_label() == *f);
                let hz_ok = self
                    .hz_filter
                    .as_ref()
                    .is_none_or(|f| format!("{:.0}", mode.refresh_hz) == *f);
                aspect_ok && hz_ok
            })
            .copied()
            .collect()
    }

    /// 将选中的显示器切换到指定模式，并更新"最近操作"反馈
    fn apply_mode(&mut self, target: &DisplayModeInfo) {
        let Some(display) = self.selected.as_ref() else {
            return;
        };
        match display::apply_mode(display.id, target) {
            Ok(()) => {
                let message = format!(
                    "已切换到 {} @ {:.0} Hz",
                    target.pixel_label(),
                    target.refresh_hz
                );
                log::info!("{message}");
                self.status_message = Some(message);
                self.refresh();
            }
            Err(err) => {
                log::error!("应用模式失败: {err}");
                self.status_message = Some(format!("切换失败: {err}"));
            }
        }
    }
}

fn main_display(displays: &[DisplayInfo]) -> Option<DisplayInfo> {
    displays
        .iter()
        .find(|d| d.is_main)
        .cloned()
        .or_else(|| displays.first().cloned())
}

/// 应用消息
#[derive(Debug, Clone)]
pub enum Message {
    /// 托盘菜单命令
    Tray(TrayCommand),
    /// 切换选中的显示器
    DisplaySelected(DisplayInfo),
    /// 刷新显示器与模式列表
    Refresh,
    /// 比例筛选变更
    AspectFilterChanged(FilterOption),
    /// 刷新率筛选变更
    HertzFilterChanged(FilterOption),
    /// 应用显示模式
    ApplyMode(DisplayModeInfo),
    /// 托盘打开的主面板窗口已创建
    PanelOpened(window::Id),
    /// 某个窗口已关闭
    PanelClosed(window::Id),
}

pub fn update(state: &mut DisplaySmith, message: Message) -> Task<Message> {
    log::debug!("update 收到消息: {:?}", message);
    match message {
        Message::Tray(TrayCommand::Quit) => iced::exit(),
        Message::Tray(TrayCommand::OpenPanel) => {
            log::info!("托盘命令: 打开主面板, 当前 panel_id={:?}", state.panel_id);
            open_panel(state)
        }
        Message::DisplaySelected(info) => {
            state.select_display(&info);
            Task::none()
        }
        Message::Refresh => {
            state.refresh();
            Task::none()
        }
        Message::AspectFilterChanged(option) => {
            state.aspect_filter = option.0;
            Task::none()
        }
        Message::HertzFilterChanged(option) => {
            state.hz_filter = option.0;
            Task::none()
        }
        Message::ApplyMode(target) => {
            state.apply_mode(&target);
            Task::none()
        }
        Message::PanelOpened(id) => {
            state.panel_id = Some(id);
            Task::none()
        }
        Message::PanelClosed(id) => {
            if state.panel_id == Some(id) {
                state.panel_id = None;
            }
            Task::none()
        }
    }
}

/// 托盘"打开主面板"：未打开则新开窗口，已打开则聚焦
fn open_panel(state: &mut DisplaySmith) -> Task<Message> {
    if let Some(id) = state.panel_id {
        return window::gain_focus(id);
    }
    let (id, task) = window::open(window::Settings {
        size: iced::Size::new(920.0, 620.0),
        min_size: Some(iced::Size::new(740.0, 480.0)),
        exit_on_close_request: false,
        ..Default::default()
    });
    state.panel_id = Some(id);
    task.map(Message::PanelOpened)
}

/// 订阅：托盘菜单命令 + 窗口关闭事件
pub fn subscription(_state: &DisplaySmith) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(tray::tray_stream).map(Message::Tray),
        window::close_events().map(Message::PanelClosed),
    ])
}

pub fn view<'a>(state: &'a DisplaySmith) -> Element<'a, Message> {
    column![
        top_bar(state),
        status_card(state),
        mode_section(state),
        footer(state),
    ]
    .spacing(10)
    .padding(14)
    .into()
}

/// 顶部工具栏：标题靠左，显示器选择与刷新靠右
fn top_bar<'a>(state: &'a DisplaySmith) -> Element<'a, Message> {
    row![
        text("DisplaySmith").size(20),
        space::horizontal(),
        text("显示器:"),
        pick_list(
            state.displays.clone(),
            state.selected.clone(),
            Message::DisplaySelected
        )
        .width(190),
        button("刷新").on_press(Message::Refresh),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into()
}

/// 当前显示器状态卡
fn status_card<'a>(state: &'a DisplaySmith) -> Element<'a, Message> {
    let Some(display) = state.selected.as_ref() else {
        return text("未检测到显示器").into();
    };
    let current = &display.current;

    column![
        row![
            text(display.title.clone()).size(16),
            if display.is_main {
                text("主显示器").size(12).color(COLOR_INFO)
            } else {
                text("")
            },
            if display.is_builtin {
                text("内建").size(12).color(COLOR_OK)
            } else {
                text("")
            },
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        row![
            text(format!(
                "{}x{} (像素 {}x{}) · {} · {:.0} Hz",
                current.logical_width,
                current.logical_height,
                current.pixel_width,
                current.pixel_height,
                current.aspect_label(),
                current.refresh_hz
            )),
            if current.is_hidpi() {
                text("HiDPI").size(12).color(COLOR_OK)
            } else {
                text("")
            },
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        text(format!("位置: ({}, {})", display.origin.0, display.origin.1)).size(13),
    ]
    .spacing(4)
    .into()
}

/// 模式列表区：标题 + 筛选下拉 + 数据表
fn mode_section<'a>(state: &'a DisplaySmith) -> Element<'a, Message> {
    let filtered = state.filtered_modes();
    let count_label = if filtered.len() == state.modes.len() {
        format!("可用显示模式（{} 个）", state.modes.len())
    } else {
        format!("可用显示模式（{}/{} 个）", filtered.len(), state.modes.len())
    };

    let aspect_options = std::iter::once(FilterOption(None))
        .chain(
            collect_aspect_options(&state.modes)
                .into_iter()
                .map(|s| FilterOption(Some(s))),
        )
        .collect::<Vec<_>>();
    let hz_options = std::iter::once(FilterOption(None))
        .chain(
            collect_hz_options(&state.modes)
                .into_iter()
                .map(|s| FilterOption(Some(s))),
        )
        .collect::<Vec<_>>();

    column![
        row![
            text(count_label).size(15),
            space::horizontal(),
            text("比例:"),
            pick_list(
                aspect_options,
                state.aspect_filter.clone().map(|value| FilterOption(Some(value))),
                Message::AspectFilterChanged
            )
            .width(120),
            text("刷新率:"),
            pick_list(
                hz_options,
                state.hz_filter.clone().map(|value| FilterOption(Some(value))),
                Message::HertzFilterChanged
            )
            .width(100),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        scrollable(
            table(
                [
                    table::column(text("像素分辨率"), |mode: DisplayModeInfo| text(
                        mode.pixel_label()
                    )),
                    table::column(text("逻辑分辨率"), |mode: DisplayModeInfo| text(format!(
                        "{}x{}",
                        mode.logical_width, mode.logical_height
                    ))),
                    table::column(text("比例"), |mode: DisplayModeInfo| text(
                        mode.aspect_label()
                    )),
                    table::column(text("刷新率"), |mode: DisplayModeInfo| text(format!(
                        "{:.0} Hz",
                        mode.refresh_hz
                    ))),
                    table::column(text("缩放"), |mode: DisplayModeInfo| if mode.is_hidpi() {
                        text("HiDPI").size(12).color(COLOR_OK)
                    } else {
                        text("—")
                    }),
                    table::column(text("操作"), |mode: DisplayModeInfo| button("应用")
                        .on_press(Message::ApplyMode(mode))),
                ],
                filtered,
            )
            .width(iced::Fill)
        )
        .height(iced::Fill),
    ]
    .spacing(6)
    .height(iced::Fill)
    .into()
}

/// 底部状态栏：统计信息 + 最近操作反馈
fn footer<'a>(state: &'a DisplaySmith) -> Element<'a, Message> {
    row![
        text(format!(
            "{} 台显示器 · {} 个可用模式",
            state.displays.len(),
            state.modes.len()
        ))
        .size(12),
        space::horizontal(),
        match &state.status_message {
            Some(message) => text(message.clone()).size(12).color(COLOR_OK),
            None => text(""),
        },
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// 收集比例筛选选项：按宽高比数值升序去重（竖屏比例自然排最前）
fn collect_aspect_options(modes: &[DisplayModeInfo]) -> Vec<String> {
    let mut seen: Vec<(f64, String)> = Vec::new();
    for mode in modes {
        if mode.pixel_width == 0 || mode.pixel_height == 0 {
            continue;
        }
        let label = mode.aspect_label();
        let ratio = mode.pixel_width as f64 / mode.pixel_height as f64;
        if !seen.iter().any(|(_, existing)| *existing == label) {
            seen.push((ratio, label));
        }
    }
    seen.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    seen.into_iter().map(|(_, label)| label).collect()
}

/// 收集刷新率筛选选项：按显示文本去重（59.94 与 60.0 同显 "60"），数值降序
fn collect_hz_options(modes: &[DisplayModeInfo]) -> Vec<String> {
    let mut rates: Vec<f64> = modes.iter().map(|mode| mode.refresh_hz).collect();
    rates.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let mut labels: Vec<String> = Vec::new();
    for hz in rates {
        let label = format!("{hz:.0}");
        if labels.last() != Some(&label) {
            labels.push(label);
        }
    }
    labels
}
