# Spec Delta

## Purpose

定义 MiniAEC 如何把用户单独安装的 VB-CABLE pair 作为 Windows 外部音频桥，将处理后的 PCM 提供给普通录音和会议客户端。

## ADDED Requirements

### Requirement: 用户拥有的外部前置条件
产品 SHALL 要求用户在 MiniAEC 外部获取、安装、更新、授权和移除 VB-CABLE，并指向 VB-Audio 官方来源；仓库可以保留官方安装包供本地调试或构建后的手动安装，但 release 不得包含该安装包，MiniAEC 产品代码不得自动下载、安装、更新、卸载、授权或改名 VB-CABLE。

#### Scenario: 未安装 VB-CABLE
- **WHEN** preflight 找不到一个受支持的 VB-CABLE pair
- **THEN** 以可操作的前置条件失败，说明官方来源，不下载软件、不请求提权、不改变驱动、设备、证书、启动配置或默认音频状态

#### Scenario: 安装或移除需要重启
- **WHEN** 外部安装器要求 Windows 重启
- **THEN** MiniAEC 不发起、计划或调用重启、关机或注销，由用户决定并手动执行

### Requirement: VB-CABLE 配对信号路径
产品 SHALL 把处理结果写入通常显示为 `CABLE Input` 的 VB-CABLE 播放端，把通常显示为 `CABLE Output` 的配对录音端作为用户在下游选择的麦克风，不得声称拥有或命名这些端点。

#### Scenario: 可用的 pair
- **WHEN** preflight 解析出唯一的活动 VB-CABLE 播放端和配对录音端
- **THEN** 报告两个精确身份，并显示 `MiniAEC -> CABLE Input -> CABLE Output -> target application` 路径

#### Scenario: 客户端选择录音端
- **WHEN** 普通录音或会议客户端读取 `CABLE Output`，同时 MiniAEC 写入配对的 `CABLE Input`
- **THEN** 客户端收到 MiniAEC 输出，不需要访问私有 MiniAEC 驱动接口

### Requirement: 确定性的端点发现和选择
产品 SHALL 按 Windows endpoint identity、data-flow role 和 pair 证据解析 VB-CABLE，启动前要求唯一且活动的 pair，不得只因友好名称包含 `CABLE` 就选择，也不得静默回退到默认端点。

#### Scenario: 解析出唯一 pair
- **WHEN** 配置的身份解析为活动播放端和配对录音端，且角色正确
- **THEN** preflight 成功并记录用于本次 run 的精确身份和显示元数据

#### Scenario: pair 缺失或歧义
- **WHEN** 端点缺失、非活动、角色错误、无法确定配对，或存在多个未明确选择的候选
- **THEN** 在启动采集或输出前失败，并报告需要用户处理的身份

### Requirement: 反馈安全的输入分离
产品 SHALL 拒绝把配对的 `CABLE Output` 当物理麦克风，也拒绝把配对的 `CABLE Input` 当 AEC 物理 render loopback。

#### Scenario: 录音端被选为麦克风
- **WHEN** 物理麦克风配置等于配对的 `CABLE Output`
- **THEN** 在打开采集、AEC 或输出前以反馈风险错误失败

#### Scenario: 播放端被选为 render 参考
- **WHEN** AEC render 配置等于配对的 `CABLE Input`
- **THEN** 在打开 loopback、AEC 或输出前以反馈风险错误失败

### Requirement: 有界格式适配和输出
产品 SHALL 从引擎输出边界接收完整的 10 ms/48 kHz mono 帧，在需要时确定性转换为 VB-CABLE 播放格式，并通过有界存储和有限等待输出，不积累无界延迟。

#### Scenario: 端点接受引擎格式
- **WHEN** VB-CABLE 播放端接受 48 kHz mono
- **THEN** 按顺序输出完整帧，不做多余采样率转换

#### Scenario: 端点需要其他 mix format
- **WHEN** VB-CABLE 播放端暴露受支持但不同于引擎表示的格式
- **THEN** 在输出边界完成确定性的声道/样本格式转换，保持时长、顺序和有限值

#### Scenario: 输出跟不上
- **WHEN** VB-CABLE 在有界队列和时序策略内无法消费输出
- **THEN** 保持 freshest-audio，记录 overflow/discard，并进入规定的 degraded 或 failed 状态，不无限增长延迟

### Requirement: 干净的停止、失败和重启
产品 SHALL 在停止或输出路径失败时停止提交、关闭 VB-CABLE render session，清空部分/转换/排队 PCM；之后的显式 restart 必须创建新 session，不提交上一 run 的 PCM。

#### Scenario: 客户端仍在读取时停止
- **WHEN** 运行中的 MiniAEC 收到 stop
- **THEN** 关闭 `CABLE Input` render session，清空保留 PCM，不再提交上一 run 的旧音频

#### Scenario: VB-CABLE 不可用
- **WHEN** 选定播放端失效或 render 流失败
- **THEN** 停止当前 run，清空输出状态，报告输出错误，要求显式 restart，不选择其他端点

#### Scenario: 显式重启
- **WHEN** 上一个 run 完全关闭且同一 pair 可用时重新启动
- **THEN** 在新 session 中恢复输出，不渲染上一 run 的缓存或转换 PCM

### Requirement: 仅元数据的输出诊断
产品 SHALL 提供解释 VB-CABLE 选择、格式协商、render 进度、队列压力、转换、失效、失败和重启的 metadata，不记录 PCM 或会议内容。

#### Scenario: 请求输出快照
- **WHEN** 控制器请求运行快照
- **THEN** 报告播放/录音身份、活动格式、已输出帧、队列深度和高水位、overflow/discard、转换、invalidation、失败和最后错误，不含 PCM

### Requirement: 普通客户端产品验收
项目 SHALL 让 Windows Recorder 和至少一个目标会议客户端从 `CABLE Output` 读取，同时 MiniAEC 写入 `CABLE Input`，以验收 bypass 和冻结默认 AEC；自动测试不得安装、更新或移除 VB-CABLE，也不得修改驱动、证书、启动配置或默认角色。

#### Scenario: 普通客户端消费产品路径
- **WHEN** 在已安装的受支持 VB-CABLE pair 上执行批准的运行时验证
- **THEN** Windows Recorder 和目标会议客户端收到新鲜的 MiniAEC 流，无无法解释的间断、旧音频或原始麦克风回退

#### Scenario: 自动验证缺少前置条件
- **WHEN** 自动测试没有已安装的受支持 VB-CABLE pair
- **THEN** 使用 fake output 或给出可操作前置条件错误，不下载、安装或改变系统音频状态
