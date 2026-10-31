//! 中文字体加载：egui 默认字体不含 CJK 字形，需运行时从系统字体注入。
//!
//! 策略：按候选顺序探测系统字体文件，命中第一个即注入为 fallback——
//! 拉丁字符仍使用 egui 自带字体，中文回落到系统字体，不向包内塞任何字体资源。

use std::fs;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

/// 候选字体按优先级排列：苹方（简体）> 冬青黑体简中 > 宋体 > 黑体 > Arial Unicode
const FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/Supplemental/Songti.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
];

/// 注入中文字体。返回是否成功加载（用于 UI 提示）。
pub fn install_chinese_fallback(ctx: &Context) -> bool {
    let Some((path, data)) = FONT_CANDIDATES.iter().find_map(|path| {
        fs::read(path).ok().map(|data| (*path, data))
    }) else {
        log::warn!("未找到任何系统中文字体，界面中文可能显示为方块");
        return false;
    };

    let mut fonts = FontDefinitions::default();
    // ttc 集合默认取第 0 个 face；PingFang.ttc 的 0 号 face 为简体中文（PingFang SC）。
    // 若个别字形异常，可通过 FontData 的 index 字段切换 face。
    fonts
        .font_data
        .insert("cjk_fallback".to_owned(), FontData::from_owned(data).into());
    // 追加到家族列表末尾，作为 fallback；不影响默认拉丁字体的渲染与度量
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("Proportional 家族必然存在")
        .push("cjk_fallback".to_owned());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .expect("Monospace 家族必然存在")
        .push("cjk_fallback".to_owned());
    ctx.set_fonts(fonts);

    log::info!("中文字体已加载: {path}");
    true
}
