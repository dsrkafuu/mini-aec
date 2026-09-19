# MiniAEC 长时音频稳定性

本文件定义当前 VB-CABLE 产品路径的 metadata-only 长时证据和 30 分钟门禁。历史 SysVAD evidence 只用于兼容读取和审计，不作为当前产品验收输入。

## 验收路径

```text
K7 物理麦克风 + 当前物理扬声器 render loopback
  -> 有界 QPC 同步 -> 冻结的 WebRTC M131 AEC3
  -> CABLE Input -> CABLE Output -> 普通 Windows 客户端
```

VB-CABLE 必须由用户从 [VB-Audio 官方页面](https://vb-audio.com/Cable/) 获取并安装。MiniAEC 不安装、更新、移除或重启外部驱动。

## 证据格式

当前实时命令写出 schema version 3 的 `engine.jsonl`：`started`、零个或多个 `periodic`，以及 `final` 或 `failed`。事件包含：

- 精确的物理麦克风、物理 render 和 VB-CABLE pair 身份；
- run、session、AEC instance、同步 epoch、单调时间和 requested duration；
- 两路 device position/QPC、同步 delta、配对、静音参考、stale、discontinuity 和 queue 计数；
- AEC 恢复、处理时间、deadline miss、输出格式、padding、render clock、转换、underrun、拒绝写入、invalidation 和 failure；
- 生命周期、降级原因和最后一个项目错误。

证据不包含 PCM 或会议内容，默认写在被 Git 忽略的 `artifacts/`。schema v1/v2 的历史 SysVAD 文件可以读取，但不能通过当前 VB-CABLE 30 分钟门禁。

## 漂移分析

分析器只使用 clean segment。以下情况会切断 segment：endpoint、run/session/AEC 或同步 epoch 改变；discontinuity/timestamp error 增加；device position/QPC 不递增；periodic 间隔超过两秒；或 render clock 没有推进。Render 静音可以是合法运行状态，但没有 render clock 样本时不能用于漂移计算。

有效 segment 按五分钟窗口分析，每个窗口至少 295 秒和 250 个 observation。相对漂移按归一化后的 render/microphone frame rate 计算，窗口中位数作为 run-level 估计；至少三个窗口中 80% 同向且超出不确定度后才算 persistent。

漂移结果只有三类：

- `bounded-synchronizer-sufficient`：数据充分，漂移不会在门禁期间耗尽 5 ms 配对容差，也没有同方向的持续同步维护。
- `clock-drift-compensation-required`：保守漂移在门禁内达到 5 ms，或同方向的 stale/silent-reference 维护重复出现，或最终同步失败有明确时钟证据。
- `inconclusive`：覆盖不足、clean duration 不足、窗口不足、方向冲突、不确定度跨越边界或计数证据矛盾。

单个 stale frame、render silence、启动恢复或无法定位到窗口的 counter 不能单独证明漂移，也不能直接授权实现补偿。

## 功能稳定性

功能门禁和漂移结论独立。分析器检查实际时长、terminal state、identity continuity、队列 overflow/discard、discontinuity/reset、AEC invalid output、deadline miss、output failure/rejected write、underrun/invalidation 和停止后的残留队列。

普通客户端是否持续消费必须由用户提供私有 operator sidecar 说明，例如 `client_continuously_consumed`、`render_active_during_scored_interval`、`no_stale_replay_or_unexplained_interruption`、`explained_counters` 和 `notes`。缺少 sidecar 时功能结果是 `inconclusive`，不会默认通过。

当功能条件通过但 render 活跃覆盖低于漂移分析阈值时，结果可以是功能 `passed`、漂移 `inconclusive`；这不构成长期漂移或补偿结论。terminal failure、输出写入失败、invalid output、deadline miss、持续增长的队列或用户明确否定始终使功能门禁失败。

## 30 分钟门禁

有效门禁要求 schema v3、请求时长至少 30 分钟、periodic 覆盖至少 95%、clean usable duration 至少请求时长的 5/6，并至少有一个 10 分钟 clean segment。30 分钟是当前产品的最终时长门禁，不自动升级为两小时。

当功能稳定性通过且漂移为 `bounded-synchronizer-sufficient` 时，当前路径通过本 change 的长时门禁；漂移为 `inconclusive` 时只能记录功能结果并补充证据；漂移要求补偿时必须创建独立 OpenSpec change。

## 运行和分析

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- realtime-aec `
  --microphone-id "<physical-capture-endpoint-id>" `
  --render-id "<physical-render-endpoint-id>" `
  --cable-input-id "<cable-playback-endpoint-id>" `
  --cable-output-id "<cable-recording-endpoint-id>" `
  --duration 1800
```

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- stability-report `
  --events "artifacts\<run>\engine-aec\<timestamp>\engine.jsonl" `
  --operator-observations "artifacts\<run>\operator-observations.json" `
  --software-revision "<git-revision>"
```

报告默认写在 event 文件旁。分析过程只读设备和证据，不联网、不改系统状态，也不发起重启。
