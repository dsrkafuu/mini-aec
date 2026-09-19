# MiniAEC AEC 基线

## 当前基线

MiniAEC 只有一个有效的 AEC 基线：

| 层 | 固定值 |
| --- | --- |
| Rust API | `webrtc-audio-processing 2.1.0` |
| Native 构建 | vendored `webrtc-audio-processing-sys 2.1.0` |
| C++ 分发 | FreeDesktop `webrtc-audio-processing 2.1` |
| 算法版本 | WebRTC M131 |
| AEC | 完整 echo canceller，上游 AEC3 默认参数 |
| NS / AGC / 后处理 | 全部关闭 |

离线工具和实时 `DefaultEchoCanceller` 都使用 `Processor::new(48_000)`。实时 adapter 先提交 render，再处理 capture；不设置 stream delay，不开放产品调参，不导出线性预抑制信号，并在 `EchoCanceller` 边界拒绝非有限输出。

依赖来源和 Windows 构建适配见 [`vendor/UPSTREAM.md`](../vendor/UPSTREAM.md)。

## 已确认的事实

- WASAPI 可以同时采集物理麦克风和物理播放回环。
- 两路数据可以用首包 QPC 时间戳放到共同的 48 kHz 时间线。
- M131 默认 AEC3 在 K7 麦克风和当前扬声器环境中可以收敛并消除可理解的远端回声。
- 当前实时路径已经把处理结果写入 `CABLE Input`，由 `CABLE Output` 提供给普通客户端。

这些事实支持保留采集、对齐、报告和默认 AEC 代码，但不等于所有房间、设备或会议客户端都已通过质量验收。

## 已知限制和历史边界

冻结默认 AEC 的双讲仍可能吞掉部分近端语音；这属于已记录的算法质量限制，不是输出链路故障，也不在普通功能变更中调参。旧 SysVAD、`MiniAEC Microphone`、测试签名和驱动回滚只属于历史证据，当前产品不再依赖它们。

## 离线诊断

列出设备：

```powershell
cargo run -p mini-aec-lab -- devices
```

采集物理麦克风和播放回环：

```powershell
cargo run -p mini-aec-lab -- capture `
  --duration 30 `
  --microphone "K7" `
  --render "Sound Blaster X4"
```

处理采集结果：

```powershell
cargo run -p mini-aec-lab -- aec --run artifacts/runs/<run-id>
```

输出位于 `processed/aec-default-adaptive/`，包括对齐后的麦克风、render reference、AEC 输出和报告。离线 WAV 只用于诊断，不替代实时 `CABLE Input -> CABLE Output` 验收。可使用 `--stream-delay-ms 60` 做固定延迟对照，但不得把它当作产品调参。

## 实时验收门槛

实时默认基线必须同时满足：用户已从官方来源安装受支持的 VB-CABLE；MiniAEC 使用精确的 `CABLE Input`/`CABLE Output` ID；物理 render loopback 进入冻结的 AEC3；普通客户端持续读取 `CABLE Output`；并记录远端单讲、近端单讲、双讲、render 静音/恢复、停止/重启和长时运行。

| 场景 | 主要检查 |
| --- | --- |
| 远端单讲 | 收敛后不出现可理解的回声语音 |
| 近端单讲 | 语音自然，字首字尾完整，电平稳定 |
| 双讲 | 近端语音可理解，无明显吞音或泵动 |
| Render 静音 | 不产生不必要的麦克风染色或旧音频 |
| 设备重启 | 有界静音后干净恢复，不重复旧帧 |
| 长时运行 | 延迟、队列、underrun 和时钟证据稳定 |

任何新的算法实验都必须先有可重复的默认基线问题，再用完全相同的输入只改变一个机制，并同时比较远端回声、收敛、近端语音、运行时间和失败行为。
