# MiniAEC 技术方案

状态：当前产品路线是 VB-CABLE 输出；旧 SysVAD 和生产驱动路线已退出产品。

目标平台：Windows 11 x64。
技术栈：Rust、无窗口 Tauri 2 托盘、WASAPI、WebRTC AEC3 M131。
产品路径：

```text
物理麦克风 + 物理播放回环 -> QPC 对齐 -> 默认 AEC3 -> CABLE Input -> CABLE Output -> 普通客户端
```

## 1. 产品边界

MiniAEC 只做声学回声消除。降噪、自动增益、均衡、去混响和语音增强由下游负责。MiniAEC 不拥有或发布 Windows 音频驱动，不在产品代码中自动下载、安装、更新、卸载、授权或改名 VB-CABLE，不提供 WebView、设置窗口或跨平台支持。

用户从 VB-Audio 官方来源单独安装和管理 VB-CABLE。MiniAEC 写入 `CABLE Input`，录音和会议客户端选择配对的 `CABLE Output`；两者都是外部设备名称，不是 MiniAEC 的自有端点。

## 2. 组件边界

- `src-tauri/`：无窗口托盘，只负责生命周期、低频控制和状态展示。
- `crates/mini-aec-engine/`：独立于 Tauri 的双输入实时引擎、同步、AEC 调用、输出边界和故障状态。
- `crates/mini-aec-output/`：平台无关的输出 session 合同。
- `crates/mini-aec-windows-output/`：VB-CABLE endpoint 校验、格式转换和 WASAPI render。
- `crates/mini-aec-lab/`：设备枚举、离线 AEC、headless bypass、实时验证和稳定性分析。
- `vendor/`：固定的 WebRTC 构建层；来源和本地补丁见 `vendor/UPSTREAM.md`。

项目级边界不暴露 WASAPI、Tauri、VB-CABLE 或 WebRTC 类型。`EchoCanceller` 保持可替换，WebRTC 类型只在 adapter 内部出现。

## 3. 音频合同

- 内部格式为 48 kHz、mono、有限的 `f32` 样本，样本范围限制在 `[-1.0, 1.0]`。
- 每帧 10 ms，即 480 个采样；输入包跨帧时保持顺序，不提交不完整帧。
- 物理麦克风和物理播放回环必须使用精确 endpoint ID；名称只用于诊断，不跟随 Windows 默认设备。
- 输出适配器接受完整 10 ms 帧，在 `CABLE Input` 的实际 mix format 边界完成确定性的声道和采样格式转换。
- 实时线程使用有界队列和有限等待，不执行文件 I/O、控制台 I/O 或 UI runtime 等待。

## 4. 实时处理和生命周期

1. 打开并校验物理麦克风、物理 render loopback 和 VB-CABLE pair。
2. 将两路输入归一化到共同 QPC 时间线；render frame 先于 capture frame 交给 AEC。
3. 检查非有限输出，必要时输出新静音并在有界策略内重建 AEC。
4. 将处理后的完整帧交给 `CABLE Input` 的 event-driven shared-mode WASAPI render。
5. `Stopped`、`Starting`、`RunningAec`、`RunningBypass`、`Degraded`、`Stopping` 和 `Failed` 状态只通过显式控制改变。

物理端点失效、VB-CABLE pair 缺失或歧义、输出初始化/写入失败、持续同步失败或 AEC 恢复失败都会终止当前 run，清空部分帧、队列、转换和处理结果，进入 `Failed`。后续必须显式 restart，并创建新的 run、同步、AEC 和输出 session；不得回退到默认设备、扬声器、原始麦克风或其他线缆。

显式 bypass 是用户选择的独立模式，不是 AEC 故障时的隐式回退。bypass 不创建 AEC，仍然必须使用精确的物理麦克风和 VB-CABLE pair。

## 5. AEC 基线

当前基线是 `webrtc-audio-processing 2.1.0`、FreeDesktop WebRTC M131 和上游 AEC3 默认参数。使用 `Processor::new(48_000)`，只启用完整 AEC；NS、AGC、实验性配置、EQ、去混响和产品后处理均关闭。具体来源和构建适配见 `vendor/UPSTREAM.md`。

## 6. 诊断和验证

- 运行事件和报告只记录 endpoint、run/session、格式、时间戳、同步、队列、AEC、转换、输出和故障元数据，不记录 PCM 或会议内容。
- 当前产品验收面是 `CABLE Input -> CABLE Output`，不是 WAV，也不是历史 `MiniAEC Microphone`。
- Windows Recorder、Discord 和会议客户端由用户手动操作；代理只运行 MiniAEC 命令并读取 metadata。
- 自动检查不得安装、更新或删除 VB-CABLE，不得修改证书、BCD、设备、默认音频角色或系统重启状态。
- 算法变更必须处理相同输入并比较远端回声、收敛、双讲语音、耗时和失败行为。

稳定性门禁至少覆盖 30 分钟。功能稳定性和时钟漂移分开判断；功能条件通过但 render 活跃覆盖不足时，可以得到功能通过、漂移 `inconclusive`，不得据此宣称完成漂移补偿。

## 7. 仓库和历史

```text
mini-aec/
├─ src-tauri/
├─ crates/
├─ docs/
├─ openspec/specs/
├─ vendor/
├─ testdata/
└─ artifacts/        # Git 忽略的私有证据，不得隐式删除
```

M0–M4 的 SysVAD、开发签名和历史客户端验证只作为 Git 历史审计材料。当前产品不再构建、签名、安装、更新或卸载 MiniAEC 驱动。

## 8. 检查

```powershell
cargo fmt --all -- --check
.tools\cargo-webrtc.cmd test --workspace
.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
openspec validate --all --strict --no-interactive
```

`target/` 和 `.tools/` 中的构建缓存可以手动清理；`artifacts/` 仅在明确确认后清理，`vendor/VBCABLE_Driver_Pack45` 保留为本地安装材料。
