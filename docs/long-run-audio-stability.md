# MiniAEC 长时音频稳定性验证

状态：OpenSpec change `characterize-long-run-audio-stability` 的 event schema v2、report schema v2、离线分析器、合成验证与真机验收均已完成。无人值守 K7 / Realtek speakers run 通过本 change 最终的 30 分钟时长 gate：drift 为 `bounded-synchronizer-sufficient`，用户确认录音没有问题，functional 为 `passed`，`thirty_minute_accepted` 为 true。批准的开发驱动 rollback 和用户手动重启后，最终 inventory 确认 device、endpoint、package、测试证书、service 与服务注册表项均已移除，K7/Realtek 默认角色和 BCD `testsigning No` 已恢复。没有强制两小时 gate；更长验证只能由初期版本日志中的具体风险通过独立 change 触发。

## 范围

本协议测量已冻结的实时产品路径：

```text
K7 physical microphone + current active Realtek speakers render loopback
  -> bounded QPC synchronizer
  -> upstream-default WebRTC M131 AEC3
  -> MiniAEC Microphone
  -> ordinary Windows capture client
```

本 change 不实现异步重采样，不改变 5 ms 配对容差、100 ms 最大 skew 观察、50 帧持续失配终止策略、AEC3 配置、PCM 格式、driver protocol 或访问控制。若证据要求漂移补偿，必须创建独立的 `compensate-audio-clock-drift` change，并针对这里保留的基线进行相同输入和长时行为比较。

## Evidence schema version 2

`realtime-aec` 继续在非实时控制线程每秒写一条 `engine.jsonl`，事件顺序为 `started`、零个或多个 `periodic`、以及 `final` 或 `failed`。Schema version 2 保留 `unix_ms` 供人工关联，并增加：

- `monotonic_elapsed_ms`：从当前 run 起点计算的单调时间；started 固定为零；
- `requested_duration_ms`：每条事件一致的声明运行时长；
- 现有 `snapshot`：endpoint、run/session/AEC identity、两路 device position/QPC、同步、队列、AEC、处理时间和 sink metadata，不包含 PCM 或会议内容。

Schema version 1 可以用于诊断和错误解释，但缺少权威的单调时长字段，因此不能通过长时 gate。分析器拒绝不支持的 schema、混合 schema、截断 JSON、非单调 elapsed time、混合 run/session/AEC identity、变化的 endpoint identity，以及缺少 terminal event 的证据。

## Clock analysis

分析器只使用 clean segment。以下任一条件会结束当前 segment，并排除跨界区间：

- endpoint、run、sink session、AEC instance 或 synchronization epoch 改变；
- microphone 或 render 的 discontinuity/timestamp-error counter 增加；
- device position 或 QPC 不递增；
- 相邻 periodic event 间隔超过两秒；
- render position 未推进或缺少任一路 clock observation。合法 render silence 仍是有效引擎行为，但不能充当 render clock 样本。

每个 clean segment 被切成不重叠的五分钟窗口。窗口必须覆盖至少 295 秒并包含至少 250 个 observation。分析器分别用 device position 对 QPC 秒数做普通最小二乘回归，得到 microphone/render 原生有效 frame rate、斜率标准误和 residual RMS。由于两个 endpoint 的原生采样率可能不同，比较前必须把每个有效 rate 除以该 endpoint 声明的标称原生采样率。

相对漂移定义固定为：

```text
relative_ppm = ((render_rate / render_nominal_rate) / (microphone_rate / microphone_nominal_rate) - 1) * 1,000,000
```

正值表示 render clock 相对其标称采样率更快，负值表示 render clock 相对更慢。`current_delta_100ns` 定义为 `render_qpc - microphone_qpc`，因此负趋势与 render-faster rate evidence 一致，正趋势与 render-slower evidence 一致。Run-level estimate 使用窗口 ppm 中位数；窗口间波动使用 median absolute deviation。方向只有在至少三个 eligible window 中至少 80% 在扣除各自不确定度和 1 ppm 数值底线后保持同号，才算 persistent。保守漂移量从中位数绝对值扣除 median uncertainty、MAD 和 1 ppm 底线中的最大者，再换算为达到现有 5 ms 配对容差的预计时间和整个 requested duration 的预计 phase error。

30 分钟 gate 的 data-quality 条件为：event schema v2、requested duration 至少 30 分钟、正常覆盖声明时长、periodic coverage 至少 95%、clean usable duration 至少 requested duration 的 5/6（30 分钟时为 25 分钟），且至少一个 clean segment 达到 10 分钟。30 分钟是本 change 的最终时长 gate，不会自动外推为固定的更长验收时长。

## Drift dispositions

- `bounded-synchronizer-sufficient`：data quality 通过，证据未显示 persistent drift 会在 gate 内达到 5 ms，且没有与方向一致的周期性整帧维护或 synchronization failure。
- `clock-drift-compensation-required`：persistent conservative drift 在 gate 内达到 5 ms，或同方向的 stale/silent-reference maintenance 在至少三个窗口重复，或 directional clock evidence 先于 terminal synchronization failure。
- `inconclusive`：coverage/clean duration 不足、少于三个 eligible window、漂移方向不一致、不确定度跨越决策边界、或 rate 与 synchronization-delta/counter 证据冲突。

单个 stale frame、render silence、startup/recovery event 或无法定位到时间窗口的累计 counter 都不能独自证明漂移。`inconclusive` 要求改进证据或重复运行，不授权直接实现补偿。

Report schema version 2 用 `thirty_minute_accepted` 明确 drift 与 functional 两个 gate 是否共同完成当前验收，并用 `follow_up` 给出 `monitor-early-version-diagnostics`、`complete-operator-observations`、`resolve-functional-failure`、`repeat-thirty-minute-evidence` 或 `propose-clock-drift-compensation`。它不再输出两小时 eligibility，也不会把早期版本日志中的未知风险预先转换为固定时长要求。

## Functional stability disposition

功能稳定性与 drift disposition 独立。分析器检查声明/实际时长、terminal state、identity continuity、两路 queue depth/overflow/discard、discontinuity/timestamp/reset、AEC invalid output、10 ms deadline miss、sink failure/rejected write、driver overflow/discard/underrun 和 stop 后残留队列。

仅凭 JSONL 不能证明普通客户端持续消费或音频连续，因此 authoritative functional gate 还需要一个 private operator sidecar：

```json
{
  "client_continuously_consumed": true,
  "render_active_during_scored_interval": true,
  "no_stale_replay_or_unexplained_interruption": true,
  "explained_counters": [],
  "notes": "Windows Recorder or Discord consumed MiniAEC Microphone for the complete scored interval."
}
```

允许解释的 counter key 为 `microphone_queue_overflows`、`microphone_discarded_frames`、`render_queue_overflows`、`render_discarded_frames`、`input_discontinuities`、`timestamp_errors`、`alignment_resets`、`aec_resets_or_rebuilds` 和 `driver_underruns`。只要 `explained_counters` 非空，`notes` 就必须给出人工核对依据。Terminal failure、rejected sink write、sink failure、driver overflow/discard、invalid AEC output、deadline miss、连续增长的 queue 或否定的 operator observation 始终使功能 gate 失败。缺少 sidecar 时功能结果为 `inconclusive`，不会默认为通过。

## Repository verification

实现和合成测试不需要安装驱动或改变 Windows 状态：

```powershell
cargo fmt --all -- --check
.tools\cargo-webrtc.cmd test --workspace
.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
```

分析已有 private evidence：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- stability-report `
  --events "artifacts\normal-user-access\<run>\engine-aec\<timestamp>\engine.jsonl" `
  --operator-observations "artifacts\normal-user-access\<run>\operator-observations.json" `
  --software-revision "<git-revision>"
```

默认在 event 文件旁写 `stability-report.json`。输入、operator sidecar 和输出都必须位于 ignored `artifacts/` 或 `driver/windows/out/`；命令不访问 PCM、不联网，也不安装、更新、restart 或移除 driver/device，不修改证书、BCD、Windows default roles，且绝不发起系统重启。

## Thirty-minute characterization

真机 gate 之前先运行只读计划：

```powershell
driver\windows\scripts\runtime-access-validation.ps1 -Action Plan
driver\windows\scripts\validation-lifecycle.ps1 -Action Plan
```

若 `MiniAEC Microphone` development package 不在已批准且 active 的状态，必须先审阅 `driver/windows/VALIDATION.md` 中精确的 signing/install/restart/uninstall/rollback 命令并取得单独明确批准。任何需要的 Windows restart 都由用户保存工作后手动执行，agent 不得启动、安排或调用 restart、shutdown 或 sign-out。

在已批准 package active 后，从普通非提升 interactive PowerShell 运行：

```powershell
driver\windows\scripts\runtime-access-validation.ps1 -Action Aec `
  -MicrophoneId "<exact-K7-capture-endpoint-id>" `
  -RenderId "<exact-active-Realtek-speakers-render-endpoint-id>" `
  -DurationSeconds 1800
```

在整个 scored interval 使用 Windows Recorder、Discord 或目标会议客户端持续消费 `MiniAEC Microphone`，并保持足够的 intentional render activity 以获得 render-clock evidence。记录 operator sidecar 后运行 `stability-report`，检查所有 excluded interval、非零 counter、clock window、drift disposition 和 functional disposition。

若运行期间不应依赖人工启动或维持播放，可使用当前 change 提供的 runtime-only orchestration：

```powershell
driver\windows\scripts\long-run-validation.ps1 `
  -MicrophoneId "<exact-K7-capture-endpoint-id>" `
  -RenderId "<exact-active-Realtek-speakers-render-endpoint-id>" `
  -DurationSeconds 1800 `
  -FfmpegPath "<ffmpeg.exe>" `
  -FfplayPath "<ffplay.exe>"
```

脚本以 `ffplay` 循环播放本地合成 far-end 素材，以普通 FFmpeg DirectShow capture client 连续消费 `MiniAEC Microphone` 并在 ignored evidence root 保存 private FLAC，随后运行同一 non-elevated AEC harness、只停止自己启动的子进程并自动生成无 operator sidecar 的初始 report。播放器或消费端提前退出会使 orchestration 失败；engine 的 render activity 与 transport counters 仍由 schema version 2 evidence 判断。Private FLAC 仅供事后听感核对，脚本不会把进程存活伪装为 `no_stale_replay_or_unexplained_interruption=true`，因此 authoritative functional pass 仍需真实 operator sidecar。

`bounded-synchronizer-sufficient` 且 functional `passed` 即完成本 change 的 30 分钟稳定性 gate。`clock-drift-compensation-required` 必须由独立补偿 change 相对本基线指定、实现并验收；`inconclusive` 必须改善或重复 30 分钟测量。接受后的初期版本应保留可审阅的 metadata 诊断契约，只有重复同步维护、增长的 queue pressure、无法解释的 discontinuity 或其他持续日志信号才触发新的 evidence-scoped 延长验证或修正 change。

## 2026-08-12/13 K7 / Realtek speakers runs

第一次 30 分钟 run 正常结束，schema version 2 periodic coverage 为 100%，usable clock evidence 为 1,790.594 秒，最长 clean segment 同为 1,790.594 秒，并产生五个 eligible window。修正真实 evidence 暴露的 native-rate normalization 和 synchronization-delta sign 问题后，中位 relative drift 为 -2.210 ppm，MAD 为 0.049 ppm，median uncertainty 为 0.079 ppm，保守 drift 为 1.210 ppm，预计 30 分钟 phase 为 2.178 ms，预计约 4,133 秒达到 5 ms，因此 drift disposition 为 `bounded-synchronizer-sufficient`。该 run 使用的旧 binary 在实际清空 render queue 后仍于 final snapshot 保留 depth 1，导致 functional gate 失败；修复为 stop 后显式归零两路 snapshot queue depth，并加入回归测试。分析器还据真实 evidence 修复了合法 AEC rebuild segmentation、不同 endpoint 原生采样率归一化、`render_qpc - microphone_qpc` 的趋势符号，以及 started-to-final counter delta 计算。

修复后的第二次 30 分钟 run 同样正常结束，final microphone/render queue depth 均为零，且没有 driver overflow/discard 增量、sink failure、processing deadline miss 或 terminal error。但 intentional render activity 在约 729.220 秒后停止，usable duration 仅 729.220 秒，低于 1,500 秒门槛，因此 drift disposition 为 `inconclusive`。

随后加入 runtime-only 自动播放和普通 FFmpeg DirectShow capture client 编排，在不依赖人工维持 render 的情况下完成第三次 30 分钟 run。Observed duration 为 1,800.021 秒，event schema version 2 periodic coverage 为 100%，usable duration 与最长 clean segment 均为 1,798.962 秒，无 excluded interval，并产生五个 eligible window。中位 relative drift 为 -2.832 ppm，MAD 为 0.149 ppm，median uncertainty 为 0.081 ppm，保守 drift 为 1.832 ppm，预计 30 分钟 phase 为 3.297 ms，预计约 2,729 秒达到 5 ms，因此 drift disposition 为 `bounded-synchronizer-sufficient`。Final microphone/render queue depth 均为零，且没有 user-space 或 driver overflow/discard、sink failure、invalid AEC output、processing deadline miss 或 terminal error。两次 alignment reset、两次 AEC reset、两次 AEC rebuild、一个 stale render frame 和六次新增 driver underrun 全部发生在首个约 1.06 秒的启动收敛区间，之后未再增长。播放器和普通 capture client 均由机器证据证明覆盖完整 interval；最初的 FLAC 因编排器强制终止 FFmpeg 而损坏尾部，保留原文件并生成了可完整解码的 1,796.389 秒 private 恢复副本，编排器随后改为等待 FFmpeg 自然封口。分析器补充使用事件流中最早可用 driver snapshot 作为缺失专用 start snapshot 时的基线，并加入回归测试；用户核对录音后确认没有问题，operator sidecar 据此解释 bounded startup recovery。最终 report 的 functional disposition 为 `passed`、`thirty_minute_accepted` 为 true、follow-up 为 `monitor-early-version-diagnostics`。当前证据不触发 `compensate-audio-clock-drift` proposal 或延长验证。Raw JSONL、reports、operator details、endpoint identities 和录音继续仅保留在 ignored private evidence roots。

## Rollback and privacy

所有 raw JSONL、report、operator sidecar、endpoint identity 和录音都是 private local evidence，不得 stage、commit、upload 或删除。仓库只记录无 machine identity 的方法、schema 和摘要结论。

完成真机运行后必须执行单独批准的 rollback，并用只读 inventory 对比 device、endpoint、published package、certificate、service、default roles 和 TESTSIGNING。若 uninstall 或 test-signing restoration 报告 pending reboot，立即停止，由用户手动重启，然后再运行 final inventory；未完成 rollback 不能把真机 acceptance 标记为完成。

2026-08-14 的已批准 rollback 已定向移除 MiniAEC root device、published package 和指定 thumbprint 在 LocalMachine My/Root/TrustedPublisher 中的证书，并运行 `bcdedit /set testsigning off`。Post-uninstall inventory 显示服务删除仍 pending，因此 agent 按设计停止，由用户手动重启。重启后的 final read-only inventory 确认 MiniAEC device、endpoint、package、证书、`MiniAECValidation` service 与服务注册表项均不存在，K7 保持 Console/Multimedia/Communications 默认输入角色，Realtek 保持三个默认输出角色，BCD 显示 `testsigning No`。K7 的 Windows endpoint identity 在设备重建后与 pre-install baseline 不同，但物理设备、格式和默认角色一致，不构成 MiniAEC 残留或默认角色回滚失败。
