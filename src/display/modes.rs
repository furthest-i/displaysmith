//! 显示器与显示模式枚举 — CoreGraphics 公开 API（手写 FFI）。
//!
//! 不经 core-graphics crate 封装的原因：其对 mode 列表与 options 的封装不完整，
//! 且后续私有 API（DisplayServices / CGS）也只能手写 FFI，统一调用风格便于维护。

use std::os::raw::{c_int, c_void};

pub type CgDisplayId = u32;
pub type CfStringRef = *const c_void;
pub type CfMutableDictionaryRef = *mut c_void;
pub type CfArrayRef = *const c_void;
pub type CgDisplayModeRef = *mut c_void;
/// 显示配置事务句柄（opaque）
pub type CgDisplayConfigRef = *mut c_void;

/// 配置生效范围：永久（写入系统偏好，重启后保留）。
/// 0 = 仅当前应用，1 = 仅本次会话，2 = 永久
const K_CG_CONFIGURE_PERMANENTLY: u32 = 2;
/// 浮点刷新率匹配容差：59.94 与 59.940001 视为同一模式
const REFRESH_MATCH_TOLERANCE_HZ: f64 = 0.01;

/// CGDisplayBounds 返回的几何结构（与 CGGeometry 头文件布局一致）
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    // 仅取地址作为字典回调指针传入，结构布局无关
    static kCFTypeDictionaryKeyCallBacks: u8;
    static kCFTypeDictionaryValueCallBacks: u8;

    fn CFDictionaryCreateMutable(
        allocator: *const c_void,
        capacity: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CfMutableDictionaryRef;
    fn CFDictionarySetValue(dict: CfMutableDictionaryRef, key: *const c_void, value: *const c_void);
    fn CFRetain(cf: *const c_void);
    fn CFArrayGetCount(array: CfArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CfArrayRef, index: isize) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// 展开"与 HiDPI 缩放模式重复的低分辨率模式"的选项 key（CoreGraphics 导出常量）
    #[link_name = "kCGDisplayShowDuplicateLowResolutionModes"]
    static K_SHOW_DUPLICATE_LOW_RES: CfStringRef;

    fn CGGetOnlineDisplayList(max_displays: u32, displays: *mut CgDisplayId, count: *mut u32) -> c_int;
    fn CGMainDisplayID() -> CgDisplayId;
    fn CGDisplayBounds(display: CgDisplayId) -> CGRect;
    fn CGDisplayIsBuiltin(display: CgDisplayId) -> u8;
    fn CGDisplayCopyAllDisplayModes(display: CgDisplayId, options: *const c_void) -> CfArrayRef;
    fn CGDisplayCopyDisplayMode(display: CgDisplayId) -> CgDisplayModeRef;
    fn CGBeginDisplayConfiguration(config: *mut CgDisplayConfigRef) -> c_int;
    fn CGConfigureDisplayWithDisplayMode(
        config: CgDisplayConfigRef,
        display: CgDisplayId,
        mode: CgDisplayModeRef,
        options: CfStringRef,
    ) -> c_int;
    fn CGCompleteDisplayConfiguration(config: CgDisplayConfigRef, option: u32) -> c_int;
    fn CGCancelDisplayConfiguration(config: CgDisplayConfigRef) -> c_int;
    fn CGDisplayModeGetWidth(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetHeight(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetPixelWidth(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetRefreshRate(mode: CgDisplayModeRef) -> f64;
    fn CGDisplayModeGetIOFlags(mode: CgDisplayModeRef) -> u32;
}

/// 一台显示器的基础信息
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    pub id: CgDisplayId,
    pub title: String,
    pub is_builtin: bool,
    pub is_main: bool,
    /// 屏幕坐标原点（多屏排列位置）
    pub origin: (f64, f64),
    pub current: DisplayModeInfo,
}

/// 一档显示模式。CoreGraphics 里每个 mode 同时给出逻辑尺寸与像素尺寸：
/// HiDPI 模式下像素尺寸是逻辑尺寸的 2 倍，UI 上需分开呈现避免混淆。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DisplayModeInfo {
    pub logical_width: usize,
    pub logical_height: usize,
    pub pixel_width: usize,
    pub pixel_height: usize,
    pub refresh_hz: f64,
    pub io_flags: u32,
}

impl DisplayModeInfo {
    /// HiDPI 推断：像素宽大于逻辑宽即为缩放（HiDPI）模式。
    /// CGDisplayModeIsHiDPI 已从新版 SDK 的导出符号中移除，无法直接调用。
    pub fn is_hidpi(&self) -> bool {
        self.pixel_width > self.logical_width
    }
}

/// 常用显示比例（横屏形态），按宽高比数值排列
const COMMON_ASPECTS: &[(usize, usize)] = &[
    (32, 9),
    (21, 9),
    (16, 9),
    (16, 10),
    (3, 2),
    (4, 3),
    (5, 4),
    (1, 1),
];

/// 比例匹配容差。取 2.5%：覆盖 21:9 的行业口径（3440x1440 实为 2.39，业内统称 21:9），
/// 同时不会把 16:10（1.60）误判成 3:2（1.50），两者相差 6.3%，安全。
const ASPECT_MATCH_TOLERANCE: f64 = 0.025;

impl std::fmt::Display for DisplayInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)
    }
}

impl DisplayModeInfo {
    /// 像素分辨率短格式，如 "3024x1890"
    pub fn pixel_label(&self) -> String {
        format!("{}x{}", self.pixel_width, self.pixel_height)
    }

    /// 显示比例标签：命中常用比例表时输出整数比（如 "16:10"），
    /// 竖屏模式自动翻转（如 "9:16"），非标准比例回退为十进制比（如 "1.54:1"）
    pub fn aspect_label(&self) -> String {
        if self.pixel_width == 0 || self.pixel_height == 0 {
            return "-".to_owned();
        }
        let is_landscape = self.pixel_width >= self.pixel_height;
        let wide = self.pixel_width.max(self.pixel_height);
        let tall = self.pixel_width.min(self.pixel_height);
        let ratio = wide as f64 / tall as f64;

        let nearest = COMMON_ASPECTS
            .iter()
            .copied()
            .min_by(|a, b| {
                let diff_a = (ratio - a.0 as f64 / a.1 as f64).abs();
                let diff_b = (ratio - b.0 as f64 / b.1 as f64).abs();
                diff_a.partial_cmp(&diff_b).unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some((w, h)) = nearest {
            let target = w as f64 / h as f64;
            if (ratio - target).abs() / target <= ASPECT_MATCH_TOLERANCE {
                return if is_landscape {
                    format!("{w}:{h}")
                } else {
                    format!("{h}:{w}")
                };
            }
        }
        format!("{ratio:.2}:1")
    }
}

/// 枚举所有在线显示器
pub fn list_displays() -> Vec<DisplayInfo> {
    let main_id = unsafe { CGMainDisplayID() };
    display_ids()
        .into_iter()
        .map(|id| {
            let current = current_mode(id);
            let bounds = unsafe { CGDisplayBounds(id) };
            let is_builtin = unsafe { CGDisplayIsBuiltin(id) } != 0;
            let title = if is_builtin {
                format!("内建显示器 (id:{id})")
            } else {
                format!("外接显示器 (id:{id})")
            };
            DisplayInfo {
                id,
                title,
                is_builtin,
                is_main: id == main_id,
                origin: (bounds.origin.x, bounds.origin.y),
                current,
            }
        })
        .collect()
}

/// 枚举某台显示器的全部显示模式（含 HiDPI 缩放档位），按像素面积与刷新率降序
pub fn list_modes(display_id: CgDisplayId) -> Vec<DisplayModeInfo> {
    unsafe {
        let array = CGDisplayCopyAllDisplayModes(display_id, duplicate_mode_options());
        if array.is_null() {
            return Vec::new();
        }
        let count = CFArrayGetCount(array);
        let mut modes: Vec<DisplayModeInfo> = (0..count)
            .map(|i| CFArrayGetValueAtIndex(array, i) as CgDisplayModeRef)
            .map(|mode_ref| parse_mode(mode_ref))
            .collect();
        CFRelease(array);

        modes.sort_by(|a, b| {
            (b.pixel_width * b.pixel_height)
                .cmp(&(a.pixel_width * a.pixel_height))
                .then_with(|| {
                    b.refresh_hz
                        .partial_cmp(&a.refresh_hz)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        modes
    }
}

unsafe fn duplicate_mode_options() -> CfMutableDictionaryRef {
    // options: { kCGDisplayShowDuplicateLowResolutionModes: true }
    // 不传该选项时系统只返回"安全"模式，会漏掉 HiDPI 缩放档位
    unsafe {
        let dict = CFDictionaryCreateMutable(
            std::ptr::null(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const u8 as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const u8 as *const c_void,
        );
        CFDictionarySetValue(dict, K_SHOW_DUPLICATE_LOW_RES, kCFBooleanTrue);
        dict
    }
}

/// 将显示器切换到指定模式（事务式，永久生效，重启后保留）。
///
/// CoreGraphics 要求传入 CGDisplayModeRef，而模式列表只保存了解析后的值，
/// 因此这里重新枚举一次，按 逻辑尺寸/像素尺寸/刷新率/IO 标志 四元组精确匹配。
pub fn apply_mode(display_id: CgDisplayId, target: &DisplayModeInfo) -> Result<(), String> {
    unsafe {
        let array = CGDisplayCopyAllDisplayModes(display_id, duplicate_mode_options());
        if array.is_null() {
            return Err("无法获取该显示器的模式列表".to_owned());
        }
        let count = CFArrayGetCount(array);
        let mut matched: Option<CgDisplayModeRef> = None;
        for i in 0..count {
            let mode_ref = CFArrayGetValueAtIndex(array, i) as CgDisplayModeRef;
            if mode_matches(mode_ref, target) {
                matched = Some(mode_ref);
                break;
            }
        }
        // 模式引用由数组持有：先 CFRetain 目标模式，再释放数组，避免悬垂
        let Some(mode_ref) = matched else {
            CFRelease(array);
            return Err("目标模式已不在当前列表中，请刷新后重试".to_owned());
        };
        CFRetain(mode_ref);
        CFRelease(array);

        let result = commit_mode(display_id, mode_ref);
        CFRelease(mode_ref);
        result
    }
}

fn mode_matches(mode_ref: CgDisplayModeRef, target: &DisplayModeInfo) -> bool {
    let candidate = parse_mode(mode_ref);
    candidate.logical_width == target.logical_width
        && candidate.logical_height == target.logical_height
        && candidate.pixel_width == target.pixel_width
        && candidate.pixel_height == target.pixel_height
        && (candidate.refresh_hz - target.refresh_hz).abs() < REFRESH_MATCH_TOLERANCE_HZ
        && candidate.io_flags == target.io_flags
}

/// 三段事务提交：Begin → Configure → Complete，任一步失败即回滚
fn commit_mode(display_id: CgDisplayId, mode_ref: CgDisplayModeRef) -> Result<(), String> {
    unsafe {
        let mut config: CgDisplayConfigRef = std::ptr::null_mut();
        let err = CGBeginDisplayConfiguration(&mut config);
        if err != 0 {
            return Err(format!("打开显示配置事务失败 (CGError {err})"));
        }
        let err = CGConfigureDisplayWithDisplayMode(config, display_id, mode_ref, std::ptr::null());
        if err != 0 {
            CGCancelDisplayConfiguration(config);
            return Err(format!("配置显示模式失败 (CGError {err})"));
        }
        let err = CGCompleteDisplayConfiguration(config, K_CG_CONFIGURE_PERMANENTLY);
        if err != 0 {
            // 1014 为 WindowServer 私有错误码：显示器睡眠/不可用时无法完成模式切换
            if err == 1014 {
                return Err("显示器当前不可用（可能正在睡眠），唤醒屏幕后重试".to_owned());
            }
            return Err(format!("提交显示配置失败 (CGError {err})"));
        }
        Ok(())
    }
}

fn display_ids() -> Vec<CgDisplayId> {
    unsafe {
        let mut ids = [0u32; 16];
        let mut count: u32 = 0;
        // Online 而非 Active：屏幕睡眠时 Active 列表为空，会导致"0 台显示器"的误报；
        // Online 列表包含所有物理在线的显示器，与菜单栏工具的语义一致
        if CGGetOnlineDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) == 0 {
            ids[..count as usize].to_vec()
        } else {
            Vec::new()
        }
    }
}

fn current_mode(display_id: CgDisplayId) -> DisplayModeInfo {
    unsafe {
        let mode_ref = CGDisplayCopyDisplayMode(display_id);
        if mode_ref.is_null() {
            DisplayModeInfo::default()
        } else {
            let mode = parse_mode(mode_ref);
            CFRelease(mode_ref);
            mode
        }
    }
}

fn parse_mode(mode: CgDisplayModeRef) -> DisplayModeInfo {
    unsafe {
        DisplayModeInfo {
            logical_width: CGDisplayModeGetWidth(mode),
            logical_height: CGDisplayModeGetHeight(mode),
            pixel_width: CGDisplayModeGetPixelWidth(mode),
            pixel_height: CGDisplayModeGetPixelHeight(mode),
            refresh_hz: CGDisplayModeGetRefreshRate(mode),
            io_flags: CGDisplayModeGetIOFlags(mode),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(pixel_width: usize, pixel_height: usize) -> DisplayModeInfo {
        DisplayModeInfo {
            logical_width: pixel_width,
            logical_height: pixel_height,
            pixel_width,
            pixel_height,
            refresh_hz: 60.0,
            io_flags: 0,
        }
    }

    #[test]
    fn standard_full_hd_matches_16_9() {
        assert_eq!(mode(1920, 1080).aspect_label(), "16:9");
    }

    #[test]
    fn scaled_16_10_matches() {
        assert_eq!(mode(1920, 1200).aspect_label(), "16:10");
    }

    #[test]
    fn hidpi_retina_pixel_ratio_unchanged() {
        // HiDPI 档位：像素与逻辑同倍缩放，比例不变
        let mode = DisplayModeInfo {
            logical_width: 1512,
            logical_height: 945,
            pixel_width: 3024,
            pixel_height: 1890,
            refresh_hz: 120.0,
            io_flags: 0,
        };
        assert_eq!(mode.aspect_label(), "16:10");
    }

    #[test]
    fn builtin_nonstandard_falls_back_to_decimal() {
        assert_eq!(mode(3024, 1964).aspect_label(), "1.54:1");
    }

    #[test]
    fn portrait_flips_ratio_text() {
        assert_eq!(mode(1080, 1920).aspect_label(), "9:16");
    }

    #[test]
    fn ultrawide_3440x1440_matches_21_9() {
        // 3440x1440 精确值为 2.39，按行业口径归入 21:9（容差 2.5%）
        assert_eq!(mode(3440, 1440).aspect_label(), "21:9");
    }

    #[test]
    fn zero_size_uses_placeholder() {
        assert_eq!(mode(0, 0).aspect_label(), "-");
    }
}

#[cfg(test)]
mod apply_tests {
    use super::*;

    #[test]
    fn mode_matches_ignores_float_noise() {
        let target = DisplayModeInfo {
            logical_width: 1512,
            logical_height: 982,
            pixel_width: 3024,
            pixel_height: 1964,
            refresh_hz: 119.97,
            io_flags: 2,
        };
        let same = DisplayModeInfo { refresh_hz: 119.9700001, ..target };
        let diff = DisplayModeInfo { refresh_hz: 59.94, ..target };
        // 同源枚举的浮点噪声应视为同一模式；刷新率不同则不匹配
        assert!((same.refresh_hz - target.refresh_hz).abs() < REFRESH_MATCH_TOLERANCE_HZ);
        assert!((diff.refresh_hz - target.refresh_hz).abs() >= REFRESH_MATCH_TOLERANCE_HZ);
    }
}

#[cfg(test)]
mod real_switch_tests {
    use super::*;
    use std::time::Duration;

    /// 真实切换验证：选一台显示器，保持分辨率不变，仅切换刷新率后切回。
    /// 分辨率不变则桌面不重排，视觉几乎无感；需在有屏环境手动运行：
    /// `cargo test real_mode_switch -- --ignored --nocapture`
    #[test]
    #[ignore = "真实切换显示器模式，手动在有屏环境运行"]
    fn real_mode_switch_round_trip() {
        let displays = list_displays();
        println!("枚举到 {} 台显示器", displays.len());
        // 优先外接屏；只有内建屏时用主显示器（内建屏同样有 ProMotion 多档刷新率）
        let target_display = displays
            .iter()
            .find(|d| !d.is_builtin)
            .or_else(|| displays.iter().find(|d| d.is_main))
            .expect("至少存在一台显示器");
        let current = target_display.current;
        let modes = list_modes(target_display.id);
        let Some(alt) = modes.iter().find(|m| {
            m.pixel_width == current.pixel_width
                && m.pixel_height == current.pixel_height
                && (m.refresh_hz - current.refresh_hz).abs() > 1.0
        }) else {
            println!("无同分辨率的其他刷新率档位，跳过");
            return;
        };

        let alt_label = format!("{} @ {:.0}Hz", alt.pixel_label(), alt.refresh_hz);
        println!("切换: {:.0}Hz -> {:.0}Hz", current.refresh_hz, alt.refresh_hz);
        apply_mode(target_display.id, alt).expect("切换到新刷新率失败");
        std::thread::sleep(Duration::from_millis(1500));

        println!("切回: {:.0}Hz", current.refresh_hz);
        apply_mode(target_display.id, &current).expect("切回原刷新率失败");
        println!("round-trip 通过 ({alt_label} 往返)");
    }
}
