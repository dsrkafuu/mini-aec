# MiniAEC

MiniAEC 是面向 Windows 11 x64 的免提声学回声消除器。它读取用户明确选择的物理麦克风和物理播放回环，使用冻结的 WebRTC M131 AEC3 基线处理，再把结果写入用户单独安装的 VB-CABLE。

```text
物理麦克风 + 物理播放回环 -> MiniAEC AEC -> CABLE Input -> CABLE Output -> 录音或会议客户端
```

MiniAEC 只做 AEC，不做降噪、自动增益、均衡、去混响或语音增强。MiniAEC 不发布或拥有 Windows 驱动，也不在产品代码中自动下载、安装、更新、卸载、授权或改名 VB-CABLE。仓库保留官方 VB-CABLE 安装包，便于本地调试和构建后手动安装；安装包不进入 release，用户自行完成安装和管理。

## 当前状态

- 产品代码是 Rust + 无窗口 Tauri 2 托盘，支持 headless CLI；没有 WebView 或设置前端。
- 实时路径使用物理麦克风、物理 render endpoint 以及精确的 VB-CABLE `CABLE Input`/`CABLE Output` ID；首次启动会把 Windows 默认输入/输出作为明确的 `Default (...)` 菜单项解析为精确 ID，不做未展示的静默回退。
- AEC 基线为 `webrtc-audio-processing 2.1.0`、FreeDesktop WebRTC M131 和上游 AEC3 默认配置。
- 旧 SysVAD/生产驱动路线已退出当前产品，不参与构建和发布。
- 当前默认 AEC 的双讲仍有一定近端吞音，这是已知质量限制，不在本次路线中调参。

## VB-CABLE 前置条件

请从 [VB-CABLE 官方页面](https://vb-audio.com/Cable/) 获取并手动管理 VB-CABLE。MiniAEC 写入 `CABLE Input`，录音和会议客户端必须选择配对的 `CABLE Output`。安装、移除和必要的重启都由用户完成；MiniAEC 不修改 Windows 默认音频角色。

当前兼容性目标是官方 `VBCABLE_Driver_Pack45.zip`（2024 年 10 月）。Windows 10/11 x64 INF `vbMmeCable64_win10.inf` 的驱动版本为 `3.3.1.7`，签名者应为 Microsoft Windows Hardware Compatibility Publisher；其他包需要重新做兼容性验证。仓库中的 `vendor/VBCABLE_Driver_Pack45` 是本地调试和构建后手动安装材料，不进入 MiniAEC release。

## 仓库结构

```text
src-tauri/                    无窗口 Tauri 托盘宿主
crates/mini-aec-lab/          采集、离线 AEC 和验证 CLI
crates/mini-aec-engine/       与 Tauri 无关的实时引擎
crates/mini-aec-output/       输出 session 合同
crates/mini-aec-windows-output/ VB-CABLE 发现和 WASAPI render
docs/                         架构、基线和验证文档
openspec/specs/               当前产品规格
vendor/                       固定的 WebRTC 构建层
artifacts/                    被 Git 忽略的私有证据，不得隐式删除
```

## 托盘运行

首次使用时直接启动托盘，不设置环境变量：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec
```

托盘菜单固定为 `Status: <STATE>`、`Enable AEC3`、`Input Microphone`、`Output Reference`、`VB-CABLE Pairs` 和 `Quit MiniAEC`。首次启动会选择 `Default (...)` 系统输入、`Default (...)` 系统输出和第一个有效 VB-CABLE pair，默认关闭 AEC3 并自动运行 bypass；AEC3 需要物理 render，bypass 只需要物理麦克风和 VB-CABLE pair。选择器和 AEC3 开关修改后会自动停止旧 run、校验、保存并应用新 run，不提供手动保存、刷新、启动、停止或重启菜单项。

配置会以版本化 JSON 保存到当前 Windows 用户的 Tauri 应用配置目录，保存模式、精确 endpoint ID 以及物理输入/输出的 Default/direct 选择意图，不包含 PCM、会议内容或 VB-CABLE 授权信息。选择 `Default (...)` 后重启会重新解析当前系统默认设备；直接选择设备则保持固定 endpoint。设备重装或失效时保持停止并报告原因，不按友好名称回退。

开发或诊断时也可以用完整的四变量组临时覆盖持久化配置：

```powershell
$env:MINI_AEC_MICROPHONE_ID = "<physical-capture-endpoint-id>"
$env:MINI_AEC_RENDER_ID = "<physical-render-endpoint-id>"
$env:MINI_AEC_CABLE_INPUT_ID = "<cable-playback-endpoint-id>"
$env:MINI_AEC_CABLE_OUTPUT_ID = "<cable-recording-endpoint-id>"
cargo run -p mini-aec
```

环境变量必须完整提供，优先于用户配置，只在当前进程生效且不会写回托盘配置；部分提供或为空会直接报告配置错误。托盘只负责低频控制和状态，不处理 PCM。状态只使用 `ONLINE`、`STARTING`、`STOPPING`、`DEGRADED`、`ERROR` 和 `OFFLINE` 等大写枚举；候选缺失时相关菜单置灰且不启动音频 run。

## 诊断和实时验证

列出设备：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- devices --json
```

运行实时 AEC：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- realtime-aec `
  --microphone-id "<physical-capture-endpoint-id>" `
  --render-id "<physical-render-endpoint-id>" `
  --cable-input-id "<cable-playback-endpoint-id>" `
  --cable-output-id "<cable-recording-endpoint-id>" `
  --duration 300
```

运行显式 bypass：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- bypass `
  --microphone-id "<physical-capture-endpoint-id>" `
  --cable-input-id "<cable-playback-endpoint-id>" `
  --cable-output-id "<cable-recording-endpoint-id>" `
  --duration 300
```

命令只写 metadata 到被忽略的 `artifacts/`，不写 PCM，不安装或改变驱动，不改证书、BCD、设备和默认音频角色。`CABLE Output` 不能作为物理麦克风，`CABLE Input` 不能作为物理 render 参考；缺失、歧义或失效时直接失败，不自动回退。

## 离线基线

```powershell
cargo run -p mini-aec-lab -- devices
cargo run -p mini-aec-lab -- capture --duration 30 --microphone "K7" --render "Sound Blaster X4"
cargo run -p mini-aec-lab -- aec --run artifacts/runs/<run-id>
```

离线 WAV 只用于诊断，不替代 `CABLE Input -> CABLE Output` 的产品验收。

## 文档和构建

- 产品边界和架构：[docs/technical-plan.md](docs/technical-plan.md)
- AEC 基线：[docs/aec-baseline.md](docs/aec-baseline.md)
- 实时验证：[docs/realtime-aec-validation.md](docs/realtime-aec-validation.md)
- 长时稳定性：[docs/long-run-audio-stability.md](docs/long-run-audio-stability.md)
- 上游来源：[vendor/UPSTREAM.md](vendor/UPSTREAM.md)
- 上游升级：[docs/upstream-upgrade-plan.md](docs/upstream-upgrade-plan.md)
- 当前规格：[openspec/specs/](openspec/specs/)

Windows 构建需要 x64 Visual Studio C++、Meson、Ninja 和 libclang。检查命令：

```powershell
cargo fmt --all -- --check
.tools\cargo-webrtc.cmd test --workspace
.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
```

`target/` 和 `.tools/` 中的本地构建缓存可以手动清理；`artifacts/` 只在明确确认后清理，`vendor/VBCABLE_Driver_Pack45` 作为本地安装材料保留。
