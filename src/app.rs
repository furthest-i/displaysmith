//! 主窗口 UI：显示器概览 + 可用模式列表 + 底部说明栏 + 托盘命令处理。
//!
//! eframe 0.37 的 App trait 拆成两个回调：
//! - `logic`：窗口隐藏时也会被调用，托盘命令轮询放在这里，保证隐藏后仍可响应菜单；
//! - `ui`：每次重绘调用，负责全部界面绘制。

use eframe::egui::{self, Color32, Context, RichText};

use crate::display::{self, DisplayInfo, DisplayModeInfo};
use crate::fonts;
use crate::tray::{Tray, TrayCommand};

/// 托盘命令轮询间隔。静态界面下以极低频率唤醒，等待菜单栏点击。
const TRAY_POLL_INTERVAL_MS: u64 = 200;

pub struct DisplaySmithApp {
    tray: Option<Tray>,
    displays: Vec<DisplayInfo>,
    modes: Vec<DisplayModeInfo>,
    selected: usize,
    fonts_ok: bool,
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
                ui.separator();
                ui.label("显示器:");
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
                if ui.button("刷新").clicked() {
                    self.refresh();
                }
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
            ui.horizontal_wrapped(|ui| {
                ui.strong(&display.title);
                if display.is_main {
                    badge(ui, "主显示器", Color32::from_rgb(96, 165, 250));
                }
                if display.is_builtin {
                    badge(ui, "内建", Color32::from_rgb(134, 200, 134));
                }
            });
            ui.add_space(2.0);
            let current = &display.current;
            ui.monospace(format!(
                "当前: {}x{} (像素 {}x{}) · {} @ {:.0} Hz",
                current.logical_width,
                current.logical_height,
                current.pixel_width,
                current.pixel_height,
                current.aspect_label(),
                current.refresh_hz
            ));
            ui.horizontal(|ui| {
                ui.label(format!("位置: ({}, {})", display.origin.0, display.origin.1));
                if current.is_hidpi() {
                    badge(ui, "HiDPI", Color32::from_rgb(134, 200, 134));
                }
            });
        });
    }

    fn draw_mode_table(&self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.strong(format!("可用显示模式（{} 个）", self.modes.len()));
        ui.add_space(2.0);

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for mode in &self.modes {
                ui.horizontal(|ui| {
                    ui.monospace(mode.pixel_label());
                    ui.weak(format!("逻辑 {}x{}", mode.logical_width, mode.logical_height));
                    ui.weak(mode.aspect_label());
                    ui.monospace(format!("{:.0}Hz", mode.refresh_hz));
                    if mode.is_hidpi() {
                        badge(ui, "HiDPI", Color32::from_rgb(134, 200, 134));
                    }
                    if ui.button("应用").clicked() {
                        // 下一阶段实现：CGBeginDisplayConfiguration 事务提交
                        log::info!("应用模式 {} (待实现)", mode.pixel_label());
                    }
                });
                ui.separator();
            }
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
                    ui.small("「应用」按钮将在私有 API 阶段接入");
                });
            });
        });
    }
}

impl eframe::App for DisplaySmithApp {
    /// 窗口隐藏时也会被调用：托盘命令轮询 + 低频唤醒兜底
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(TRAY_POLL_INTERVAL_MS));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw_top_bar(ui);
        self.draw_main(ui);
        self.draw_footer(ui);
    }
}

/// 徽章样式标签（egui 0.37 移除了内置 badge，此处自行封装）
fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.22))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().color(color));
        });
}
