# Spec Delta

## MODIFIED Requirements

### Requirement: 明确的物理 render-loopback 参考
系统 SHALL 要求每个 AEC run 使用精确且活动的物理 render endpoint，确实捕获该端点的 loopback，拒绝选定的 `CABLE Input`，不得跟随 Windows 默认 render 或替换其他端点。

#### Scenario: 打开明确的 render
- **WHEN** AEC run 的物理 render ID 活动且不同于 VB-CABLE 播放端点
- **THEN** 引擎只为该物理端点打开 loopback，并报告身份和显示元数据

#### Scenario: 配置的 render 不可用
- **WHEN** 物理 render 缺失、非活动或不能提供 loopback
- **THEN** AEC 启动失败，不打开其他 render，也不发布健康运行状态

#### Scenario: 运行中的 render 改变
- **WHEN** 物理 render 失效或 loopback 流失败
- **THEN** 停止当前 run，清空未配对和处理 PCM，关闭 VB-CABLE 输出，并报告需要显式 restart 的 render 错误

### Requirement: 安全的 AEC 降级和恢复
系统 SHALL 区分健康 AEC、显式 bypass、临时 AEC 降级和终止失败；AEC 结果非法时提交新静音，不得因 render、同步或 AEC 失败而静默发送原始麦克风帧。

#### Scenario: render 参考暂时缺失
- **WHEN** 有界的 render 短缺发生在有效 AEC run 中
- **THEN** 进入或保持可见的 degraded 状态，使用定义的静音参考，并在恢复门槛满足后返回健康 AEC

#### Scenario: AEC 返回错误或非有限输出
- **WHEN** adapter 处理失败或产生非有限样本
- **THEN** 对该时段提交新静音，记录失败并重置或重建 adapter，不发送原始麦克风帧

#### Scenario: AEC 重建成功
- **WHEN** adapter 在有界恢复策略内重建且后续对齐处理成功
- **THEN** 以新的 AEC 状态回到 `RunningAec`，不保留重置前队列

#### Scenario: AEC 恢复失败
- **WHEN** adapter 无法重建或连续失败超过上限
- **THEN** 关闭 VB-CABLE 输出，清空缓存，进入 `Failed`，要求显式 restart

### Requirement: 有界实时 AEC 路径
系统 SHALL 对麦克风、render、同步、AEC 和 VB-CABLE 输出使用有界存储和有限等待；实时 worker 不做文件/控制台 I/O 或 UI 等待，并暴露 deadline、格式转换和队列压力证据。

#### Scenario: 实时处理跟得上
- **WHEN** 两路物理输入和 VB-CABLE 输出健康
- **THEN** 每个有效 10 ms 区间按序处理和提交，不产生无界分配、队列增长或无法解释的丢帧

#### Scenario: 处理超出预算
- **WHEN** AEC、转换或 VB-CABLE render 造成有界队列溢出或处理超时
- **THEN** 保持 freshest-audio 行为，记录 overflow/deadline miss，并进入定义的 degraded 或 failed 状态，不积累延迟

#### Scenario: 写入验证证据
- **WHEN** headless controller 写周期或最终 AEC 证据
- **THEN** 文件 I/O 在实时 worker 外执行，内容只有 metadata

### Requirement: AEC 专用元数据诊断
系统 SHALL 提供足以解释物理 render、同步、AEC 健康、处理耗时、VB-CABLE 输出、降级和恢复的有界 metadata，不包含 PCM 或会议内容。

#### Scenario: 请求 AEC 快照
- **WHEN** 控制器在 AEC run 期间或之后请求快照
- **THEN** 报告物理麦克风、物理 render、VB-CABLE pair、各角色 packet/frame/silence/discontinuity/timestamp、同步 delta、配对、静音参考、stale、重置、skew、AEC 处理/重建/非法输出、处理时间、输出格式和最后错误

#### Scenario: 托盘读取状态
- **WHEN** 无窗口托盘刷新低频状态
- **THEN** 只读取项目状态和 metadata，并区分 stopped、starting、running AEC、degraded、explicit bypass、缺少 VB-CABLE 和 failed，不访问 PCM 或 WebRTC 类型

### Requirement: 默认 AEC 端到端功能和质量验收
项目 SHALL 通过普通客户端从 `CABLE Output` 消费、同时由 MiniAEC 写入 `CABLE Input` 来验收冻结的 M131 AEC3 和声学场景；要记录默认基线质量缺陷而不宣称通过。自动测试不得安装或修改 VB-CABLE、驱动、证书、启动配置、设备或默认角色；算法优化必须是独立 change 并使用相同输入的新旧证据。

#### Scenario: 远端单讲
- **WHEN** 选定扬声器的播放回环进入 AEC，并通过物理麦克风返回
- **THEN** `CABLE Output` 收到的流在收敛后没有可理解的远端回声语音，metadata 能解释同步、重置、underrun、处理和输出

#### Scenario: 近端单讲
- **WHEN** 扬声器静音且近端语音进入物理麦克风
- **THEN** `CABLE Output` 保留自然、可理解的语音，字首字尾完整，无持续泵动或染色

#### Scenario: 双讲
- **WHEN** 近端语音在收敛前后与扬声器播放重叠
- **THEN** 记录近端语音在无明显吞音/泵动下是否达到目标，同时保留远端回声判断
- **THEN** 若目标未达成，将其记录为默认算法限制，不否定已通过的输出、生命周期、安全和客户端消费

#### Scenario: render 静音和重启
- **WHEN** 播放静音或按批准流程重启任一物理输入
- **THEN** 暴露有界 degraded 静音或干净恢复，建立新的同步/AEC epoch，不提交旧 PCM

#### Scenario: 普通客户端消费
- **WHEN** Windows Recorder 和至少一个目标会议应用在批准的 AEC 验证中读取 `CABLE Output`
- **THEN** 收到预期时长的处理流，无无法解释的间断、旧片段、原始麦克风回退或 MiniAEC 自有公共端点

#### Scenario: 没有系统前置条件
- **WHEN** 自动检查或机器运行没有已安装的 VB-CABLE pair
- **THEN** 使用 fake 输入、fake AEC 和 fake 输出，或给出可操作前置条件错误，不改变 Windows 状态
