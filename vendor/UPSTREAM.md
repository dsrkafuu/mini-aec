# WebRTC AEC 上游来源

本文件记录 MiniAEC 使用的 AEC 快照、构建适配和许可证。更新依赖或本地补丁时必须同步修改本文件。

## 当前固定版本

| 层 | 版本或提交 | 来源 | 用途 |
| --- | --- | --- | --- |
| Rust API | `webrtc-audio-processing 2.1.0` | crates.io | 安全 Rust processor API |
| Rust 配置 | `webrtc-audio-processing-config 2.1.0` | crates.io | APM 配置类型 |
| Rust FFI/构建 | `webrtc-audio-processing-sys 2.1.0` | `vendor/webrtc-audio-processing-sys` | C++ bridge 和 Windows 构建层 |
| Rust 上游提交 | `c14d7af1760baff83e8210fee336a0cae0faaa7d` | 发布包 `.cargo_vcs_info.json` | wrapper 来源 |
| C++ 分发包 | FreeDesktop `webrtc-audio-processing 2.1` | vendored `-sys` crate | APM 源码和 Meson 构建 |
| 算法基线 | WebRTC M131 | FreeDesktop 发布元数据 | AEC3 实现 |

Rust wrapper 没有本地修改；Cargo 固定在 2.1.0，只把 `webrtc-audio-processing-sys` 替换为项目维护的 Windows 构建层。发布包不包含完整的 FreeDesktop/Google Git 元数据，因此无法从快照还原更深层的精确提交；M131 是当前最强的算法固定点。

## 来源链

```text
Google WebRTC M131 AEC3
  -> FreeDesktop webrtc-audio-processing 2.1
  -> tonarino webrtc-audio-processing-sys 2.1.0
  -> crates.io webrtc-audio-processing 2.1.0
  -> MiniAEC QPC 对齐和 10 ms adapter
```

算法源码位于 `vendor/webrtc-audio-processing-sys/webrtc-audio-processing/webrtc/modules/audio_processing/aec3/`。MiniAEC 不修改这些 AEC3 算法文件。

## 本地构建适配

本地修改只在 vendored FFI/构建层：

1. MSVC 使用 C++20 构建 WebRTC 和 wrapper，以支持源码中的 designated initializer。
2. MSVC 构建跳过 GCC 专用 warning flags。
3. 为 bindgen/libclang 解析当前 Visual Studio 头文件提供兼容宏。
4. MSVC 禁用 archive symbol prefixing，避免 LLVM objcopy 破坏 C++ 未定义引用。
5. 使用 Cargo 的 verbatim native library 语法链接 MSVC 静态库。
6. 用 Rust `fs_extra` 复制源码，避免 Windows 依赖 Unix `cp`。
7. 为 wrapper 和 bindgen 设置 `WEBRTC_WIN`、`NOMINMAX`。

这些修改只处理构建和链接；旧的线性 AEC getter 与实验性配置已删除。

## 许可证

- Rust API wrapper：BSD-3-Clause。
- vendored FFI/build wrapper：BSD-3-Clause，见其 `COPYING`。
- FreeDesktop 包：BSD 风格许可证，见其 `COPYING`。
- Google WebRTC：BSD 风格许可证和 `PATENTS` 授权。
- 第三方组件继续保留自己的许可证、专利和作者声明文件。

刷新 vendor 时不得删除这些通知文件。

## 更新规则

Google WebRTC `main` 变化不会自动触发升级。只有 `docs/upstream-upgrade-plan.md` 中的触发条件满足，并且相同输入回归门禁通过，才允许更新快照。

优先顺序是：有明确 WebRTC milestone 和提交的稳定 FreeDesktop 版本、匹配的稳定 Rust wrapper，最后才是因已测量产品阻塞而批准的项目自有 Google WebRTC 快照。每次候选升级都要记录精确提交、逐项重放本地补丁、更新许可证和 Cargo pin，并完成构建、测试和声学回归。
