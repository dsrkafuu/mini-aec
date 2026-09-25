# MiniAEC

MiniAEC 是面向 Windows 11 x64 的免提声学回声消除托盘应用。

[English](README.md)

## 它解决什么问题

MiniAEC 读取物理麦克风和物理音响播放参考，使用 AEC3 处理麦克风信号，再把结果写入 VB-CABLE：

```text
物理麦克风 + 音响播放参考
              -> MiniAEC AEC
              -> CABLE Input -> CABLE Output -> 录音或会议应用
```

应用运行在 Windows 托盘中，设备选择和 AEC 状态会自动应用，不需要设置窗口、手动保存或刷新。

## 开始前准备

- Windows 11 x64。
- 从 [VB-Audio 官方来源](https://vb-audio.com/Cable/) 自行安装和管理 VB-CABLE。
- 把录音或会议应用的输入设置为 `CABLE Output`。

MiniAEC 不捆绑、下载、安装、更新、移除、授权或改名 VB-CABLE，也不会请求系统重启。发布包不包含 VB-CABLE 安装包或 Windows 驱动文件。所需 pair 不存在时，MiniAEC 保持离线或报告错误，不会静默切换到其他设备。

## 快速开始

1. 自行安装 VB-CABLE，并完成它要求的 Windows 操作。
2. 启动 MiniAEC，在托盘菜单中选择物理麦克风、物理输出参考和 VB-CABLE pair；默认项和第一个有效项会自动提供。
3. 旁路使用时保持 AEC 关闭；有可用的物理输出参考后再启用 AEC3。
4. 在录音或会议应用中选择 `CABLE Output`。

源码构建和发布包说明见 [Windows 发布与开发说明](docs/windows-release.md) 和[技术方案](docs/technical-plan.md)。

## 范围与限制

- 只做 AEC：不做降噪、自动增益、均衡、去混响或语音增强。
- 使用用户态 Rust/Tauri 托盘宿主；不拥有 MiniAEC 自有 Windows 音频驱动。
- 当前 AEC3 基线固定，用于可复现开发。
- 项目不提供正式生产代码签名证书；发布产物可能未签名，Windows 可能显示安全提示。

## 文档

- [AEC 基线](docs/aec-baseline.md)
- [实时验证](docs/realtime-aec-validation.md)
- [长时稳定性](docs/long-run-audio-stability.md)
- [上游升级计划](docs/upstream-upgrade-plan.md)
- [OpenSpec 变更与产品契约](openspec/README.md)

详细的 WASAPI、endpoint 身份、诊断、工具链和验证规则放在 `docs/` 中，不在项目首页展开。

## 开发

仓库是 Rust workspace，包含无窗口的 Tauri 2 托盘宿主。在 Windows 11 x64 上，通过 mise 提供 Rust、Meson 和 Ninja，并准备含 C++ 桌面开发工具、LLVM/Clang 的 Visual Studio Build Tools。在仓库根目录运行 `mise exec -- .\.tools\cargo-webrtc.cmd build --release`；其他检查见 [Windows 发布与开发说明](docs/windows-release.md)。
