//! 显示器能力模块。
//!
//! 分阶段实现：
//! 1. 枚举（本模块，CoreGraphics 公开 API）
//! 2. 应用模式（下一阶段，CGBeginDisplayConfiguration 事务）
//! 3. 自定义分辨率（再下一阶段，私有 API / 虚拟显示器）

pub mod modes;

pub use modes::{list_displays, list_modes, DisplayInfo, DisplayModeInfo};
