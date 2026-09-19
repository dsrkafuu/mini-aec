# Spec Delta

## MODIFIED Requirements

### Requirement: 独立的实时生命周期
系统 SHALL 提供不依赖 CLI、WebView 或异步 UI 的 Tauri 独立实时引擎，支持一个物理麦克风 bypass run 或一个双输入 AEC run，并支持 start、stop 和显式 restart。

#### Scenario: AEC 引擎正常启动
- **WHEN** 停止状态的引擎收到有效的物理麦克风、物理 render loopback 和 VB-CABLE pair 配置
- **THEN** 状态经过 `Starting` 进入 `RunningAec`，并报告输入、输出、run、同步、AEC 和 output-session 身份

#### Scenario: Bypass 引擎正常启动
- **WHEN** 停止状态的引擎收到有效的物理麦克风和 VB-CABLE pair bypass 配置
- **THEN** 状态进入 `RunningBypass`，且不打开 render loopback 或创建 AEC

#### Scenario: 引擎正常停止
- **WHEN** 运行或降级状态的引擎收到 stop
- **THEN** 停止接受输入，关闭输入、AEC 和 VB-CABLE 资源，清空部分帧、队列、转换和处理 PCM，并进入 `Stopped`

#### Scenario: 引擎显式重启
- **WHEN** 已停止或失败的引擎在上一个 run 关闭后收到新的 start
- **THEN** 创建新的 run、同步、AEC（如适用）和 output-session 身份，且不暴露上一 run 的 PCM

### Requirement: 物理输入隔离
系统 SHALL 要求每个物理输入角色使用精确 Windows endpoint ID，启动前报告显示元数据，拒绝把配对的 `CABLE Output` 当物理麦克风或把 `CABLE Input` 当物理 render-loopback，并且不得回退到默认或其他端点。

#### Scenario: 明确选择物理麦克风和 render
- **WHEN** AEC 配置解析出一个活动物理 capture、一个不同的活动物理 render 和一个有效 VB-CABLE pair
- **THEN** 引擎只打开这些角色对应的端点，并报告全部输入和输出身份

#### Scenario: 明确选择 bypass 输入
- **WHEN** bypass 配置解析出一个不同于 `CABLE Output` 的活动物理 capture
- **THEN** 只打开该 capture，不要求也不打开物理 render

#### Scenario: 递归选择 MiniAEC 输出
- **WHEN** 物理麦克风配置解析为配对的 `CABLE Output`
- **THEN** 在打开 render、AEC 或输出前，以可操作的 invalid-source 错误失败

#### Scenario: 把 VB-CABLE 播放端点当 AEC 参考
- **WHEN** 物理 render 配置解析为配对的 `CABLE Input`
- **THEN** 在打开麦克风、loopback、AEC 或输出前，以反馈风险错误失败

#### Scenario: 配置的输入不可用
- **WHEN** 所需端点缺失、非活动或数据流角色错误
- **THEN** 不打开其他端点，也不改变 Windows 默认设备策略，直接失败

### Requirement: 固定格式归一化和分帧
系统 SHALL 把事件驱动的物理麦克风包转换为有限的 48 kHz mono 样本，跨任意包边界组装精确的 10 ms/480-sample 帧，只把完整帧提交给输出边界；端点格式转换留在采集和 AEC 合同之外。

#### Scenario: 数据包跨帧边界
- **WHEN** 输入包的帧数不能整除 480
- **THEN** 保持样本顺序，每个完整帧只输出一次

#### Scenario: 输入包标记静音
- **WHEN** WASAPI 将输入包标记为静音
- **THEN** 为该时长填入新生成的零样本，不复用之前的麦克风 PCM

#### Scenario: 输入包含非法样本
- **WHEN** 归一化遇到 NaN、无穷或超出 `[-1.0, 1.0]` 的有限值
- **THEN** 非有限值替换为零，有限值限幅，并记录清理结果，不提交非法样本

#### Scenario: 输入报告 discontinuity
- **WHEN** 输入在部分帧缓存期间报告 discontinuity 或 timestamp error
- **THEN** 记录事件并清空部分帧，避免跨 discontinuity 拼帧

### Requirement: 显式 bypass 输出
在 `RunningBypass` 中，系统 SHALL 把归一化的物理麦克风帧直接送入一个 VB-CABLE output session，明确报告 bypass 状态，不调用 WebRTC AEC3，也不把 bypass 当作 AEC 故障回退。

#### Scenario: Bypass 正常运行
- **WHEN** 物理麦克风和 VB-CABLE 输出健康且状态为 `RunningBypass`
- **THEN** 普通客户端从配对的 `CABLE Output` 收到按 capture 时钟排列的麦克风帧，快照明确报告 bypass

#### Scenario: Bypass session 建立
- **WHEN** 新 bypass run 打开选定的 VB-CABLE 播放端点
- **THEN** 建立新的 output session，转换状态和 render 队列为空

### Requirement: 故障隔离和旧音频防护
系统 SHALL 将物理设备失效、不可恢复 WASAPI 错误、VB-CABLE 缺失或歧义、输出访问失败和拒绝写入视为当前 run 的终止故障，清空所有 PCM，并要求显式 restart，不得静默换端点或继续输出原始音频。

#### Scenario: 物理输入运行中失效
- **WHEN** 选定麦克风失效或 WASAPI 流失败
- **THEN** 关闭输出 session、清空缓存、进入 `Failed` 并报告输入错误，不选择其他麦克风

#### Scenario: 虚拟输出运行中失效
- **WHEN** 选定 VB-CABLE 播放端点消失或 render 流失败
- **THEN** 停止采集、关闭剩余资源、清空缓存、进入 `Failed` 并报告映射后的输出错误

#### Scenario: 故障后重启
- **WHEN** 操作者在故障后显式 restart 且输入与 pair 恢复可用
- **THEN** 以新的 run 和 output session 恢复，且不提交故障前 PCM

### Requirement: 仅元数据的引擎诊断
系统 SHALL 暴露包含生命周期、模式、输入角色、VB-CABLE 身份、分帧、同步、AEC、队列、转换和输出计数的有界快照与事件，并且日志不得包含 PCM 或会议内容。

#### Scenario: 请求 bypass 快照
- **WHEN** 控制器请求 bypass 快照
- **THEN** 报告 bypass、麦克风、VB-CABLE pair、run/session、采集、静音、discontinuity、timestamp、归一化、队列、overflow/discard、输出、转换和失败信息，不虚构 render 或 AEC 活动

#### Scenario: 请求 AEC 快照
- **WHEN** 控制器请求 AEC 快照
- **THEN** 在 bypass 字段之外报告物理 render、同步/skew、AEC 处理/恢复、降级原因和 AEC 状态

#### Scenario: 持久化诊断
- **WHEN** headless 验证命令写周期事件或最终结果
- **THEN** 只在实时线程外写入被忽略的 metadata 路径，不写 PCM 或会议内容

### Requirement: 端到端 bypass 验收
项目 SHALL 提供 headless 验证路径：使用明确物理麦克风，把音频写入已安装的 VB-CABLE 播放端点，并通过配对录音端点验证结果；不得下载、安装、更新或删除 VB-CABLE，也不得修改驱动、证书、启动配置、设备或默认角色。

#### Scenario: 连续 bypass 成功
- **WHEN** Windows Recorder 至少录制五分钟的 `CABLE Output`，同时引擎把物理麦克风 bypass 到 `CABLE Input`
- **THEN** 录音时长正确、无旧片段和无法解释的间断，诊断解释 discontinuity、underrun、overflow、discard、转换和输出失败

#### Scenario: 验证停止并重启
- **WHEN** 按批准的流程停止引擎并开始新的 bypass run
- **THEN** 从新的 output session 恢复，不能重放停止前的麦克风 PCM

#### Scenario: 没有驱动前置条件
- **WHEN** 自动检查或未批准的机器验证没有已安装的 VB-CABLE pair
- **THEN** 使用 fake output 或给出可操作的前置条件错误，不下载软件或改变 Windows 状态
