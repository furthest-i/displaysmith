# DisplaySmith（显界）

macOS 显示器分辨率管理工具。菜单栏常驻 + 设置窗口，目标：小体积、低资源占用。

技术栈：Rust + eframe/egui（glow 后端）+ tray-icon，显示器控制走 CoreGraphics。

## 功能状态

| 功能 | 状态 | 说明 |
| --- | --- | --- |
| 显示器识别 | ✅ 已实现 | 内建/外接识别、主屏判断、位置坐标 |
| 分辨率列表 | ✅ 已实现 | 含 HiDPI 缩放档位（`kCGDisplayShowDuplicateLowResolutionModes`） |
| 切换显示器 | ✅ 已实现 | 下拉选择，模式列表联动加载 |
| 应用模式 | ⏳ 下一阶段 | `CGBeginDisplayConfiguration` 事务提交 |
| 自定义分辨率 | ⏳ 规划中 | DisplayServices / CGVirtualDisplay 私有 API |
| 菜单栏弹出面板 | ⏳ 规划中 | 当前为菜单 + 常驻窗口形态 |

## 构建

```bash
cargo build --release   # 发布构建
cargo run               # 开发运行（会弹出窗口与菜单栏图标）
```

依赖：macOS 11+，Xcode Command Line Tools（链接 CoreGraphics / CoreFoundation framework）。

## 结构

```
src/
├── main.rs          # 入口：日志初始化 + eframe 启动（glow 后端）
├── app.rs           # 主窗口 UI + 托盘命令处理
├── tray.rs          # 菜单栏托盘（图标代码光栅化，无图片资源）
├── fonts.rs         # 中文字体注入（系统字体 fallback，不打包字体文件）
└── display/
    ├── mod.rs       # 模块导出
    └── modes.rs     # CoreGraphics FFI：显示器/模式枚举
```

## 关键决策记录

- **glow 而非 wgpu**：二进制体积约小 8 MiB，2D 界面场景 OpenGL 足够；macOS 上 OpenGL 已弃用但可用。
- **手写 CoreGraphics FFI**：`core-graphics` crate 对 mode 列表 options 封装不完整；后续私有 API
  （DisplayServices / CGS）也只能手写 FFI，统一风格。
- **中文字体**：egui 默认字体无 CJK 字形。运行时按 `PingFang.ttc → Hiragino Sans GB.ttc → Songti.ttc →
  STHeiti → Arial Unicode` 顺序探测系统字体注入 fallback，不向安装包塞字体资源。
  注意：macOS 27 上 PingFang.ttc 读取未命中，实测回退到 Hiragino Sans GB，渲染正常。
- **HiDPI 推断**：`CGDisplayModeIsHiDPI` / `CGDisplayModeIsStretched` 已从新版 macOS SDK 的
  CoreGraphics 导出符号中移除，无法链接。改用 `pixel_width > logical_width` 推断缩放档位。
- **托盘事件模型**：菜单命令由 `App::logic` 每帧 try_recv 轮询（logic 在窗口隐藏时仍被调用），
  通过 `ViewportCommand` 在主线程作用窗口，规避跨线程 Send 问题。
- **egui 0.37 UI API 变化**：App trait 的 `update` 拆分为 `logic` + `ui(ui)`；
  面板统一为 `egui::Panel`，且 `show` 挂在 `Ui` 上；内置 badge 被移除，自行封装。

## 下一阶段 TODO

1. 应用显示模式：`CGBeginDisplayConfiguration` / `CGConfigureDisplayWithDisplayMode` /
   `CGCompleteDisplayConfiguration`（公开 API，事务式）
2. 菜单栏点击弹出面板：取 `NSStatusItem` frame 定位窗口
3. 私有 API 探索：DisplayServices 枚举全量模式 + Sehozera 15+ 回落逻辑（参考 macdisp）
4. LSUIElement（隐藏 Dock 图标）：`cargo-bundle` 打包时配置 Info.plist
5. 显示器插拔回调：`CGDisplayRegisterReconfigurationCallback` 自动刷新列表
