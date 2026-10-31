//! 主窗口 UI：显示器概览 + 可用模式列表 + 底部说明栏 + 托盘命令处理。
//!
//! eframe 0.37 的 App trait 拆成两个回调：
//! - `logic`：窗口隐藏时也会被调用，托盘命令轮询放在这里，保证隐藏后仍可响应菜单；
//! - `ui`：每次重绘调用，负责全部界面绘制。

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Context, RichText};

use crate::display::{self, DisplayInfo, DisplayModeInfo};
use crate::fonts;
use crate::tray::{Tray, TrayCommand};

/// 托盘命令轮询间隔。静态界面下以极低频率唤醒，等待菜单栏点击。
const TRAY_POLL_INTERVAL_MS: u64 = 200;
/// 状态消息的展示时长，超时自动消失
const STATUS_MESSAGE_TTL: Duration = Duration::from_secs(5);

pub struct DisplaySmithApp {
    tray: Option<Tray>,
    displays: Vec<DisplayInfo>,
    modes: Vec<DisplayModeInfo>,
    selected: usize,
    fonts_ok: bool,
    /// 底部状态栏的临时消息（如切换结果反馈）
    status_message: Option<String>,
    status_until: Option<Instant>,
    /// 比例筛选（None = 全部），取值为 aspect_label 文本
    aspect_filter: Option<String>,
    /// 刷新率筛选（None = 全部），取值为取整后的显示文本（如 "120"）
    hz_filter: Option<String>,
}

impl DisplaySmithApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let fonts_ok = fonts::install_chinese_fallback(&cc.egui_ctx);

        let displays = display::list_displays();
        let modes = displays
            .first()
            .map(|d| display::list_modes(d.id))
            .unwrap_or_default();
        log::info!("启动完成: {} 台显示器", displays.len());

        Self {
            tray: Tray::new(),
            displays,
            modes,
            selected: 0,
            fonts_ok,
            status_message: None,
            status_until: None,
            aspect_filter: None,
            hz_filter: None,
        }
    }

    fn current_display(&self) -> Option<&DisplayInfo> {
        self.displays.get(self.selected)
    }

    /// 重新枚举显示器与当前所选显示器的模式
    fn refresh(&mut self) {
        self.displays = display::list_displays();
        if self.selected >= self.displays.len() {
            self.selected = 0;
        }
        self.modes = self
            .current_display()
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

    /// 切换到指定显示器并加载其模式列表
    fn select_display(&mut self, index: usize) {
        self.selected = index;
        self.modes = self
            .current_display()
            .map(|d| display::list_modes(d.id))
            .unwrap_or_default();
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
                let hz_ok = self.hz_filter.as_ref().is_none_or(|f| {
                    format!("{:.0}", mode.refresh_hz) == *f
                });
                aspect_ok && hz_ok
            })
            .copied()
            .collect()
    }

    /// 将当前显示器切换到指定模式，并在底部状态栏反馈结果
    fn apply_mode(&mut self, target: DisplayModeInfo) {
        let Some(display) = self.current_display() else {
            return;
        };
        let display_id = display.id;
        match display::apply_mode(display_id, &target) {
            Ok(()) => {
                let message = format!(
                    "已切换到 {} @ {:.0} Hz",
                    target.pixel_label(),
                    target.refresh_hz
                );
                log::info!("{message}");
                self.set_status_message(message);
                self.refresh();
            }
            Err(err) => {
                log::error!("应用模式失败: {err}");
                self.set_status_message(format!("切换失败: {err}"));
            }
        }
    }

    fn set_status_message(&mut self, message: String) {
        self.status_message = Some(message);
        self.status_until = Some(Instant::now() + STATUS_MESSAGE_TTL);
    }

    /// 状态消息到期后清除
    fn expire_status_message(&mut self) {
        if let Some(until) = self.status_until {
            if Instant::now() >= until {
                self.status_message = None;
                self.status_until = None;
            }
        }
    }

    fn handle_tray(&mut self, ctx: &Context) {
        let command = self.tray.as_ref().and_then(|tray| tray.poll_command());
        match command {
            Some(TrayCommand::Quit) => {
                log::info!("收到退出命令");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(TrayCommand::ToggleWindow) => {
                let visible = ctx.input(|i| i.viewport().visible().unwrap_or(true));
                if visible {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
            }
            None => {}
        }
    }

    fn draw_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("DisplaySmith");
                // 控件组推到工具栏右侧，避免与大标题混排导致基线错位
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("刷新").clicked() {
                        self.refresh();
                    }
                    let selected_text = self
                        .current_display()
                        .map(|d| d.title.clone())
                        .unwrap_or_else(|| "无显示器".to_owned());
                    let mut picked: Option<usize> = None;
                    egui::ComboBox::from_id_salt("display_select")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            for (index, display) in self.displays.iter().enumerate() {
                                if ui
                                    .selectable_value(&mut self.selected, index, &display.title)
                                    .clicked()
                                {
                                    picked = Some(index);
                                }
                            }
                        });
                    if let Some(index) = picked {
                        self.select_display(index);
                    }
                    ui.label("显示器:");
                });
            });
            ui.add_space(4.0);
        });
    }

    fn draw_main(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default_margins().show(ui, |ui| {
            self.draw_status_card(ui);
            self.draw_mode_table(ui);
        });
    }

    fn draw_status_card(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            let Some(display) = self.current_display() else {
                ui.label("未检测到显示器");
                return;
            };
            // 两列 Grid：标签列宽度统一，值列从同一横坐标起始
            egui::Grid::new("status_grid")
                .num_columns(2)
                .spacing([20.0, 3.0])
                .show(ui, |ui| {
                    ui.strong("显示器");
                    ui.horizontal(|ui| {
                        ui.strong(&display.title);
                        if display.is_main {
                            tinted(ui, "主显示器", Color32::from_rgb(96, 165, 250));
                        }
                        if display.is_builtin {
                            tinted(ui, "内建", Color32::from_rgb(134, 200, 134));
                        }
                    });
                    ui.end_row();

                    ui.strong("当前模式");
                    let current = &display.current;
                    ui.horizontal(|ui| {
                        ui.monospace(format!(
                            "{}x{} (像素 {}x{}) · {} · {:.0} Hz",
                            current.logical_width,
                            current.logical_height,
                            current.pixel_width,
                            current.pixel_height,
                            current.aspect_label(),
                            current.refresh_hz
                        ));
                        if current.is_hidpi() {
                            tinted(ui, "HiDPI", Color32::from_rgb(134, 200, 134));
                        }
                    });
                    ui.end_row();

                    ui.strong("位置");
                    ui.monospace(format!(
                        "({}, {})",
                        display.origin.0, display.origin.1
                    ));
                    ui.end_row();
                });
        });
    }

    fn draw_mode_table(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let filtered = self.filtered_modes();

        ui.horizontal(|ui| {
            let count_label = if filtered.len() == self.modes.len() {
                format!("可用显示模式（{} 个）", self.modes.len())
            } else {
                format!("可用显示模式（{}/{} 个）", filtered.len(), self.modes.len())
            };
            ui.strong(count_label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // right_to_left：先放的靠右。视觉上从左到右为 比例 -> 刷新率
                egui::ComboBox::from_id_salt("hz_filter")
                    .selected_text(
                        self.hz_filter
                            .as_ref()
                            .map(|hz| format!("{hz} Hz"))
                            .unwrap_or_else(|| "全部刷新率".to_owned()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.hz_filter, None, "全部刷新率");
                        for hz in collect_hz_options(&self.modes) {
                            ui.selectable_value(
                                &mut self.hz_filter,
                                Some(hz.clone()),
                                format!("{hz} Hz"),
                            );
                        }
                    });
                egui::ComboBox::from_id_salt("aspect_filter")
                    .selected_text(
                        self.aspect_filter
                            .clone()
                            .unwrap_or_else(|| "全部比例".to_owned()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.aspect_filter, None, "全部比例");
                        for aspect in collect_aspect_options(&self.modes) {
                            ui.selectable_value(
                                &mut self.aspect_filter,
                                Some(aspect.clone()),
                                &aspect,
                            );
                        }
                    });
            });
        });
        ui.add_space(2.0);

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            // Grid 列宽取该列最宽单元格，保证表头、数据、徽章、按钮全列对齐
            egui::Grid::new("mode_table")
                .num_columns(6)
                .spacing([16.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    draw_table_header(ui, &["像素分辨率", "逻辑分辨率", "比例", "刷新率", "缩放", "操作"]);

                    // 先收集点击、循环结束后再应用：避免迭代借用与可变刷新冲突
                    let mut clicked: Option<DisplayModeInfo> = None;
                    for mode in &filtered {
                        ui.monospace(mode.pixel_label());
                        ui.weak(format!(
                            "{}x{}",
                            mode.logical_width, mode.logical_height
                        ));
                        ui.monospace(mode.aspect_label());
                        ui.monospace(format!("{:.0} Hz", mode.refresh_hz));
                        if mode.is_hidpi() {
                            tinted(ui, "HiDPI", Color32::from_rgb(134, 200, 134));
                        } else {
                            ui.weak("—");
                        }
                        if ui.link("应用").clicked() {
                            clicked = Some(*mode);
                        }
                        ui.end_row();
                    }
                    if let Some(target) = clicked {
                        self.apply_mode(target);
                    }

                    if filtered.is_empty() {
                        ui.weak("当前筛选组合下没有模式，试试放宽条件");
                        ui.end_row();
                    }
                });
        });
    }

    fn draw_footer(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!(
                    "{} 台显示器 · {} 个可用模式 · glow 渲染",
                    self.displays.len(),
                    self.modes.len()
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.fonts_ok {
                        ui.small(RichText::new("中文字体未加载").color(Color32::from_rgb(230, 170, 70)));
                    }
                    if let Some(message) = &self.status_message {
                        ui.small(RichText::new(message).color(Color32::from_rgb(134, 200, 134)));
                    }
                });
            });
        });
    }
}

impl eframe::App for DisplaySmithApp {
    /// 窗口隐藏时也会被调用：托盘命令轮询 + 低频唤醒兜底
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        self.expire_status_message();
        ctx.request_repaint_after(std::time::Duration::from_millis(TRAY_POLL_INTERVAL_MS));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw_top_bar(ui);
        self.draw_main(ui);
        self.draw_footer(ui);
    }
}

/// 着色文本标签：高度与正文一致，避免 Frame 徽章把单元格行高撑出参差
fn tinted(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).small().color(color));
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

/// 表格表头行：逐列渲染加粗标题后收行
fn draw_table_header(ui: &mut egui::Ui, titles: &[&str]) {
    for title in titles {
        ui.strong(*title);
    }
    ui.end_row();
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::display::DisplayModeInfo;

    fn mode(pixel_width: usize, pixel_height: usize, refresh_hz: f64) -> DisplayModeInfo {
        DisplayModeInfo {
            logical_width: pixel_width,
            logical_height: pixel_height,
            pixel_width,
            pixel_height,
            refresh_hz,
            io_flags: 0,
        }
    }

    #[test]
    fn aspect_options_sorted_numerically_deduped() {
        let modes = vec![
            mode(1920, 1080, 60.0),
            mode(1920, 1200, 60.0),
            mode(1920, 1080, 120.0),   // 与第一项同比例，应去重
            mode(3024, 1964, 120.0),   // 非标准比例 1.54:1
        ];
        let options = collect_aspect_options(&modes);
        assert_eq!(options, vec!["1.54:1", "16:10", "16:9"]);
    }

    #[test]
    fn hz_options_sorted_desc_deduped_by_label() {
        let modes = vec![
            mode(100, 100, 59.94),
            mode(100, 100, 60.0),   // 与 59.94 同显 "60"，应去重
            mode(100, 100, 120.0),
            mode(100, 100, 48.0),
        ];
        let options = collect_hz_options(&modes);
        assert_eq!(options, vec!["120", "60", "48"]);
    }
}
