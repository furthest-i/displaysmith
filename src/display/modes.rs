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
    fn CFArrayGetCount(array: CfArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CfArrayRef, index: isize) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// 展开"与 HiDPI 缩放模式重复的低分辨率模式"的选项 key（CoreGraphics 导出常量）
    #[link_name = "kCGDisplayShowDuplicateLowResolutionModes"]
    static K_SHOW_DUPLICATE_LOW_RES: CfStringRef;

    fn CGGetActiveDisplayList(max_displays: u32, displays: *mut CgDisplayId, count: *mut u32) -> c_int;
    fn CGMainDisplayID() -> CgDisplayId;
    fn CGDisplayBounds(display: CgDisplayId) -> CGRect;
    fn CGDisplayIsBuiltin(display: CgDisplayId) -> u8;
    fn CGDisplayCopyAllDisplayModes(display: CgDisplayId, options: *const c_void) -> CfArrayRef;
    fn CGDisplayCopyDisplayMode(display: CgDisplayId) -> CgDisplayModeRef;
    fn CGDisplayModeGetWidth(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetHeight(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetPixelWidth(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: CgDisplayModeRef) -> usize;
    fn CGDisplayModeGetRefreshRate(mode: CgDisplayModeRef) -> f64;
    fn CGDisplayModeGetIOFlags(mode: CgDisplayModeRef) -> u32;
}

/// 一台显示器的基础信息
#[derive(Debug, Clone)]
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

impl DisplayModeInfo {
    /// 像素分辨率短格式，如 "3024x1890"
    pub fn pixel_label(&self) -> String {
        format!("{}x{}", self.pixel_width, self.pixel_height)
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
        // options: { kCGDisplayShowDuplicateLowResolutionModes: true }
        // 不传该选项时系统只返回"安全"模式，会漏掉 HiDPI 缩放档位
        let dict = CFDictionaryCreateMutable(
            std::ptr::null(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const u8 as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const u8 as *const c_void,
        );
        CFDictionarySetValue(dict, K_SHOW_DUPLICATE_LOW_RES, kCFBooleanTrue);
        let array = CGDisplayCopyAllDisplayModes(display_id, dict);
        CFRelease(dict);

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

fn display_ids() -> Vec<CgDisplayId> {
    unsafe {
        let mut ids = [0u32; 16];
        let mut count: u32 = 0;
        if CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) == 0 {
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
