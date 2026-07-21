# MiniAEC 技术方案

状态：M1 虚拟麦克风数据通路与 M2 实时 bypass 已通过开发期 elevated 路径验收；当前无活动 OpenSpec change；下一阶段为 M3 实时默认 AEC3，项目尚非可用 AEC 产品

目标平台：Windows 11 x64

应用技术栈：Rust + Tauri 2（无窗口托盘）+ WASAPI + WebRTC AEC3

驱动技术栈：Windows WDK / SysVAD 派生驱动

## 1. 产品边界

MiniAEC 只解决外放场景的声学回声：实际送往物理音响的声音经过房间和设备再次进入物理麦克风，会议对方因此听见自己的声音。

产品输入与输出固定为：

```text
物理麦克风 + 物理播放设备 loopback -> AEC -> MiniAEC Microphone
```

用户可以把 `MiniAEC Microphone` 继续交给 NVIDIA Broadcast、会议软件自带降噪或其他二级处理器。MiniAEC 本身不实现：

- 噪声抑制（NS）；
- 自动增益（AGC）；
- EQ、去混响或音色增强；
- 神经网络语音增强；
- macOS、Linux 或移动端支持；
- 设置窗口或 Web 前端。

第一个可用版本必须自带 `MiniAEC Microphone`。只有托盘壳、离线 WAV 输出或依赖第三方虚拟线缆都不能算可用版本。

## 2. 总体架构

```mermaid
flowchart LR
  Remote["会议软件远端声音"] --> Mixer["Windows Audio Engine"]
  Mixer --> Speaker["物理音响"]
  Mixer --> Loopback["WASAPI render loopback"]
  Mic["物理麦克风"] --> Capture["WASAPI capture"]
  Capture --> Normalize["格式归一化和 10 ms 分帧"]
  Loopback --> Normalize
  Normalize --> Align["时间对齐和漂移控制"]
  Align --> AEC["EchoCanceller boundary / WebRTC AEC3"]
  AEC --> Safety["有限数检查、静音和显式旁路"]
  Safety --> Bridge["VirtualMicrophoneSink boundary"]
  Bridge --> Driver["MiniAEC Microphone"]
  Driver --> Downstream["可选二级降噪或会议软件"]
  Tray["Tauri tray host"] -. 控制和状态 .-> Engine["Rust audio engine"]
  Engine --> Capture
  Engine --> Loopback
  Engine --> Bridge
```

### 2.1 Tauri 托盘宿主

Tauri 只负责进程生命周期和低频控制面：

- 当前状态；
- AEC 启用或显式旁路；
- 物理麦克风和回放设备选择；
- 重启音频引擎；
- 开机启动；
- 打开日志目录；
- 退出。

不创建 WebView 或主窗口。Tauri runtime 不处理 PCM，托盘销毁也不能意外穿透实时线程边界。

### 2.2 Rust 音频引擎

`crates/mini-aec-engine/` 已建立并负责设备生命周期、预分配缓冲、格式转换、10 ms 调度、虚拟麦克风输出和故障状态。它能脱离 Tauri 测试；render 同步和 AEC 编排保留给后续里程碑。

已建立的核心边界包括：

- `AudioInput`：当前承载显式选择的物理 capture；M3 需要把输入角色扩展到物理 render loopback，同时保持 Windows 类型不泄漏；
- `VirtualMicrophoneSink`：向驱动提交处理后 PCM；
- `EngineController` / `EngineSnapshot`：非实时控制和只读状态。

M3 将新增项目级 `EchoCanceller` 和有界双路同步边界，负责 render 分析、capture 处理、reset、故障状态和元数据诊断；WebRTC 类型只能存在于默认 M131 adapter 内。

WebRTC、WASAPI、Tauri 和驱动通信类型不能泄漏到这些项目级合同之外。

M2 的实时 bypass 合同固定为：

- 配置必须提供精确的物理 capture endpoint ID；名称只用于诊断，不能自动跟随默认设备，也不能把 `MiniAEC Microphone` 选为自身输入；
- 一个 capture worker 在自身线程创建、使用并销毁 WASAPI/COM 对象，一个 sink worker 独占一次 `VirtualMicrophoneSink` session；实时 worker 不写文件、不打印、不等待 Tauri 或 async runtime；
- WASAPI 使用 event-driven shared mode 和 Windows Audio Engine conversion 请求 48 kHz、单声道 `f32`，随后清理非有限数、限制范围并组装严格的 480-sample 帧；
- capture 到 sink 之间只有四个完整帧的同步 latest-wins ring；满时丢弃最旧未读帧，协议 sequence 在 dequeue 时从零分配，因此本地丢帧不会制造协议序号缺口；
- 生命周期为 `Stopped → Starting → RunningBypass → Stopping → Stopped`，source invalidation、不可恢复 capture 错误、driver absence、access denial、sender contention、version mismatch 或 rejected write 会终止当前 run、清空 PCM 并进入 `Failed`，只能显式 restart；
- snapshot 只包含 endpoint、run/session identity、packet/frame、silence、discontinuity、timestamp、queue、sink 和 error 元数据，不包含 PCM 或会议内容。

当前验证驱动的控制 DACL 只允许 SYSTEM 和 Administrators，因此 headless bypass 的真机端到端验证是开发期 elevated 路径。普通用户托盘如何访问正式签名驱动仍属于后续安装/权限架构，M2 不对其作推断。显式 bypass 是独立里程碑，不是 AEC 故障时静默泄漏原始麦克风的回退策略。

### 2.3 Windows 音频适配

使用 WASAPI 直接访问物理 capture、render loopback、事件驱动缓冲、设备位置和 QPC 时间戳。不能用跨平台抽象隐藏 Windows 的 loopback、设备通知或时序信息。

### 2.4 AEC 适配

当前基线为 `webrtc-audio-processing 2.1.0` 和 FreeDesktop M131 源码。只启用完整 AEC，使用 AEC3 上游默认参数；NS、AGC 和实验配置关闭。

依赖来源和本地构建修改以 [`vendor/UPSTREAM.md`](../vendor/UPSTREAM.md) 为准。升级必须遵循 [`upstream-upgrade-plan.md`](upstream-upgrade-plan.md)。

### 2.5 虚拟麦克风驱动

驱动基于固定版本的 Microsoft SysVAD，使用 WDK 所需的 C/C++。公共 capture endpoint 名称固定为 `MiniAEC Microphone`。

当前验证实现已选择受限控制设备加驱动自有有界环形缓冲。用户态通过仅允许 SYSTEM 和 Administrators 访问的非音频控制接口提交固定 10 ms PCM 帧，驱动只公开 `MiniAEC Microphone` capture endpoint；私有 WaveRT render sink 因无法可靠满足普通应用不可枚举的约束而在设计阶段排除。

控制协议固定为 48 kHz、单声道、PCM16、每帧 480 samples/960 bytes。驱动的 10 帧非分页环形缓冲不映射到用户态：欠载输出零值静音，溢出丢弃最旧未消费完整帧并保留最新帧，新会话原子清空旧 PCM。协议版本、会话 ID、单调序列、当前深度、高水位、拒绝写入、欠载、溢出和丢弃帧均可诊断。

## 3. 音频合同

- 内部采样率：48 kHz；
- 样本格式：`f32`，有限且限制在 `[-1.0, 1.0]`；
- 处理帧：10 ms，每单声道 480 samples；
- 麦克风：首期单声道；
- render reference：按 AEC 适配器要求映射声道；
- 每个输入块携带单调时间戳、设备帧位置、序列号和 discontinuity；
- 实时路径禁止文件 I/O、控制台 I/O、无界队列、等待 Tauri runtime 和不可控堆分配。

设备 mix format 不一定是 48 kHz 或 `f32`。WASAPI 层读取真实格式，归一化层负责通道映射、重采样和 10 ms 重组。

## 4. 同步与处理顺序

1. 捕获最终送往选中物理音响的 render loopback；
2. 捕获选中的物理麦克风；
3. 转换到内部格式并建立统一 QPC 时间线；
4. 先向 AEC 提交对应 render frame；
5. 再处理 capture frame；
6. 检查非有限数和输出范围；
7. 把结果写入虚拟麦克风数据通路。

麦克风和 Sound Blaster 等播放设备可能使用独立硬件时钟。短期对齐不能证明长期稳定，因此实时链路建立后必须记录 timestamp delta、buffer depth、discontinuity 和 drift。确认持续误差后再实现小比例异步重采样，不能长期靠整帧丢弃或补零维持同步。

## 5. 故障策略

| 故障 | 行为 |
| --- | --- |
| render reference 短暂不足 | 输入静音参考，停止错误自适应并记录 underrun，不阻塞 capture |
| 麦克风数据不足 | 输出对应时长静音，不重复旧音频 |
| AEC 错误或非有限数 | 当前帧静音、进入 degraded 并重建处理器；不得静默泄漏原始回声 |
| 用户明确关闭 AEC | 使用可见的 raw microphone bypass 状态 |
| 输入或回放设备变化 | 停止旧流、清空缓冲、重建流并 reset AEC |
| 用户态进程失联 | 驱动输出静音，不重复最后一帧 |
| 诊断写盘过慢 | 丢诊断帧并计数，不能阻塞实时路径 |

原始麦克风旁路只能由用户明确选择，不能作为无提示的 AEC 故障降级。

## 6. 仓库结构

```text
mini-aec/
├─ Cargo.toml
├─ crates/
│  ├─ mini-aec-lab/       # 已有：采集、QPC 对齐、默认 AEC 离线验证
│  └─ mini-aec-engine/    # 已有：M2 实时 bypass、项目级边界和 Windows capture adapter
├─ src-tauri/             # 已有：无窗口托盘宿主
├─ driver/windows/        # SysVAD 来源、驱动和安装边界
├─ docs/
├─ vendor/
└─ testdata/              # 仅可再分发且有来源/许可证的素材
```

`artifacts/` 是被 Git 忽略的私人本地录音目录，不属于可提交项目结构。

## 7. 里程碑

### M0：仓库准备基线

- 统一 MiniAEC/mini-aec 命名；
- 删除 Web 前端与 Node/Bun 工具链；
- Tauri 改为无窗口托盘；
- 离线工具改名为 `mini-aec-lab`；
- AEC 恢复 M131 默认配置；
- 删除旧 profile、盲测和线性诊断实验入口。

完成不代表产品可用。

### M1：`MiniAEC Microphone` 数据通路 spike

- 固定 SysVAD 来源和许可证；
- 生成并安装测试签名驱动；
- Windows 中只暴露预期的公共 capture endpoint；
- 从用户态连续送入确定性测试信号；
- 用 Windows 录音工具和至少一个会议软件读取；
- 验证停止、崩溃、重启和卸载。

已完成。开发期测试签名包已经验证唯一公共 capture endpoint、固定帧传输、sender/session 隔离、录音客户端消费、重启行为和完整 rollback；该结论不等同于正式签名、installer 或普通用户权限方案已经完成。

### M2：实时 bypass 链路

- 建立 `mini-aec-engine`；
- 物理麦克风实时写入 `MiniAEC Microphone`；
- 实现设备选择、状态、显式旁路和故障恢复；
- 连续运行无爆音、旧帧重复或无界延迟。

仓库内 engine、Windows capture adapter、headless harness 和合成验证已建立；开发期 elevated 真机验收已覆盖五分钟连续录音、stop/start 隔离、sender contention、设备 restart 和完整 rollback。普通用户驱动访问、正式安装与签名仍是后续工作，普通测试不得自行改变系统。

### M3：实时默认 AEC3

- 同时接入物理 render loopback；
- 10 ms QPC 对齐并运行默认 M131 AEC3；
- 托盘显示 running/degraded/bypass；
- 在真实外放、近端单讲和双讲中端到端验证。

这是当前下一阶段，尚未创建 OpenSpec change。建议 capability 为 `realtime-echo-cancellation`，change 名称为 `implement-realtime-default-aec`。该阶段只建立默认算法的实时产品链路和可观察故障行为，不包含 AEC 调参、依赖升级、长期漂移补偿、普通用户驱动权限、安装或生产签名。

### M4：漂移与稳定性

- 使用 K7 和 Sound Blaster X4 进行至少 30 分钟漂移测量；
- 根据证据实现异步重采样控制；
- 完成设备切换和两小时稳定性 gate。

### M5：安装与签名

- 协调应用与驱动安装、升级、回滚和卸载；
- 区分开发测试签名与正式发布签名；
- 完成主流会议软件兼容性矩阵。

## 8. 验证原则

算法或同步改变必须对相同输入执行旧/新处理，至少比较：

- far-end-only 的回声残留和收敛；
- near-end-only 的音色、字首和字尾；
- double-talk 的吞音、抽吸和音量稳定性；
- 端到端延迟、CPU P50/P95/P99；
- discontinuity、underrun、overrun、reset 和恢复；
- `MiniAEC Microphone` 被下游软件实际读取时的连续性。

更高的抑制量不能覆盖近端人声损伤。调参只能在默认实时产品链路出现可复现失败后开始，并且一次改变一个机制。

## 9. 隐私与安全

- 全部音频默认本地处理；
- 默认不保存录音；
- 诊断录音必须显式开启并写入 `artifacts/`；
- 日志不包含 PCM 或会议内容；
- 驱动通信接口使用最小权限和有界缓冲；
- 驱动安装、更新和卸载需要明确授权及回滚路径；
- 第三方源码、模型或测试素材必须记录版本、来源、许可证和 hash。

## 10. SDD 状态

仓库使用 OpenSpec 的 `spec-driven` schema 和 Codex 集成。M1 的 `virtual-microphone-transport` 与 `driver-development-lifecycle` capability 已 accepted，完成的 change 保存在 archive；`implement-realtime-microphone-bypass` 的 29 项任务与单独审批的 acceptance 已完成，新增的 `realtime-audio-engine` capability 已同步到主 specs，change 已归档。当前 `openspec list --json` 无活动 change；M3 必须从新 proposal 开始，不能修改已归档的 M1/M2 artifacts。
