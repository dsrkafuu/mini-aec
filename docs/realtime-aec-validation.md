# MiniAEC 实时 AEC 验证

## 目的

当前验收只针对：物理麦克风 + 物理播放回环 -> 有界 QPC 同步 -> 冻结的 WebRTC M131 AEC3 -> `CABLE Input` -> `CABLE Output`。历史 `MiniAEC Microphone`/SysVAD 结果只保留为工程证据，不作为当前产品路径或签名、安装和分发结论。

## 运行前提

- 用户从 [VB-CABLE 官方页面](https://vb-audio.com/Cable/) 手动安装受支持的 pair，并自行完成可能需要的重启。
- MiniAEC 托盘使用精确的物理麦克风、物理 render、`CABLE Input` 播放和 `CABLE Output` 录音 endpoint ID；首次启动提供 `Default (...)` 系统输入、`Default (...)` 系统输出和第一个 VB-CABLE pair，并自动运行 bypass。
- 代理只运行 MiniAEC 命令和读取 metadata；Windows Recorder、Discord、会议软件等第三方客户端由用户手动启动、配置、录制和关闭。
- 不使用 Windows 默认设备，不把友好名称当作唯一身份，不把 `CABLE Output` 当物理麦克风，不把 `CABLE Input` 当物理 render 参考。

## 托盘验证顺序

1. 启动 MiniAEC，确认菜单依次显示 `Status: <STATE>`、`Enable AEC3`、三个选择器和 `Quit MiniAEC`；有必要设备时状态自动进入 `ONLINE`。
2. 确认 `Input Microphone` 和 `Output Reference` 的第一个独立选项是系统默认项，`VB-CABLE Pairs` 默认勾选第一个 pair，且 AEC3 默认关闭。
3. 由用户选择其他物理麦克风、物理 render 或 VB-CABLE pair，确认每次选择都自动停止旧 run、自动保存并创建新的 run/session；不出现手动保存、刷新或启动要求。
4. 由用户切换 `Enable AEC3`，确认 bypass 不打开物理 render，AEC3 模式通过 render preflight 后写入 `CABLE Input`；状态使用大写枚举。
5. 由用户手动启动并配置 Windows Recorder 或目标会议客户端，确认它消费 `CABLE Output`；代理不启动、配置或关闭第三方客户端。设备失效时确认当前 run 停止且不换端点，设备恢复或重新选择后才重新应用。

开发覆盖仍可使用完整的四变量组。覆盖优先于持久化 JSON，只对当前进程有效，不能由启动流程写回配置；bypass 仍不打开物理 render，但覆盖组为保持现有 headless 兼容仍要求四个精确 ID。

## 当前实现边界

实时 adapter 使用 `Processor::new(48_000)`、完整 AEC 和上游默认 AEC3 参数；NS、AGC、实验性配置、EQ、去混响和后处理关闭。render frame 先于 capture frame 提交，非有限结果输出新静音并按有界策略重建，恢复失败进入 `Failed`，不会静默旁路原始麦克风。

输入同步使用有限队列、5 ms 配对容差和 100 ms 最大 skew 观察；持续无法配对、端点失效、输出失败或队列失控都会终止当前 run。所有 run/session/AEC/output identity 都写入 metadata，日志不包含 PCM 或会议内容。

## 设备枚举和实时命令

先列出设备并保存精确 ID：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- devices --json
```

默认 AEC：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- realtime-aec `
  --microphone-id "<physical-capture-endpoint-id>" `
  --render-id "<physical-render-endpoint-id>" `
  --cable-input-id "<cable-playback-endpoint-id>" `
  --cable-output-id "<cable-recording-endpoint-id>" `
  --duration 300
```

显式 bypass：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- bypass `
  --microphone-id "<physical-capture-endpoint-id>" `
  --cable-input-id "<cable-playback-endpoint-id>" `
  --cable-output-id "<cable-recording-endpoint-id>" `
  --duration 300
```

两条命令都会在缺失、失效、角色错误或 pair 歧义时失败，不会自动选择其他设备。输出证据写到被忽略的 `artifacts/`；实时 worker 不写文件、不打印、不等待 UI。

## 验收矩阵

| 场景 | 通过条件 |
| --- | --- |
| Bypass 五分钟 | `CABLE Output` 有预期时长，无旧音频、无无法解释的间断，非零 counter 有解释 |
| 远端单讲 | 收敛后没有可理解的回声语音 |
| 近端单讲 | 语音自然，字首字尾完整，电平稳定 |
| 双讲 | 记录近端吞音/泵动是否达到目标；不通过时标记为默认算法限制 |
| Render 静音 | 使用计数静音参考，保持 `Degraded`，不切换 bypass，不重复旧帧 |
| Render 恢复 | 达到恢复门槛后回到 `RunningAec`，无旧队列内容 |
| Stop/Restart | 新 run/session/AEC/output identity，清空所有旧 PCM |
| Source/Output invalidation | 当前 run 进入 `Failed`，不换端点，显式 restart 后才恢复 |
| 长时运行 | 见 `docs/long-run-audio-stability.md` 的功能和漂移双门禁 |

## 非变更边界

自动验证不得安装、更新、移除 VB-CABLE，不得修改驱动、证书、BCD、设备、默认角色或系统重启。`artifacts/` 中的私有录音和 endpoint metadata 不提交、不上传、不隐式删除。算法或同步变更必须处理相同输入，比较远端回声、收敛、双讲语音、运行时间和失败行为。

## 历史结果

早期 M1–M4 记录证明了 WASAPI 采集、QPC 对齐、默认 AEC、生命周期隔离、客户端消费和 30 分钟工程方法，但它们使用已退休的 SysVAD/`MiniAEC Microphone` 路线。当前 VB-CABLE 路径的真实验收只以 `CABLE Input -> CABLE Output` 的新证据为准；历史实现以 Git 历史和保留的迁移摘要为准。
