# Spec Delta

## Purpose

定义 MiniAEC 如何在无 WebView 的本地配置入口中管理精确音频 endpoint、用户态配置和托盘生命周期，让已安装 VB-CABLE 的普通用户可以安全启动、停止和重新配置 AEC。

## ADDED Requirements

### Requirement: 精确的本地设备配置

产品 SHALL 提供无 WebView 的本地配置入口，让用户为 AEC 选择一个活动的物理麦克风、一个活动的物理 render-loopback、一个 VB-CABLE `CABLE Input` 播放端和配对的 `CABLE Output` 录音端，并展示足以区分设备的元数据。

#### Scenario: 配置 AEC 路径

- **WHEN** 用户选择了活动的物理麦克风、不同的物理 render-loopback 和有效的 VB-CABLE pair
- **THEN** 配置入口保存四个精确 endpoint ID，显示 `物理麦克风 -> MiniAEC AEC -> CABLE Input -> CABLE Output` 路径，并允许进入停止状态

#### Scenario: 配置 bypass 路径

- **WHEN** 用户选择了活动的物理麦克风和有效的 VB-CABLE pair，并选择 bypass 模式
- **THEN** 配置入口保存物理麦克风与 VB-CABLE pair，明确 bypass 不使用物理 render-loopback

#### Scenario: 设备选择不完整

- **WHEN** 缺少必需 endpoint、endpoint ID 为空或配置只包含友好名称而没有精确身份
- **THEN** 配置保持未完成，不能启动音频，并报告需要补全的角色

### Requirement: 用户态配置持久化

系统 SHALL 在当前 Windows 用户范围内持久化精确 endpoint ID、模式和必要的配置版本信息；配置内容不得包含 PCM、会议内容、签名材料或 VB-CABLE 安装授权信息。

#### Scenario: 应用重新启动

- **WHEN** MiniAEC 重新启动且上一次配置文件仍然存在
- **THEN** 恢复配置并重新解析设备身份，但保持停止状态，等待用户显式启动

#### Scenario: 配置文件不可读或版本不支持

- **WHEN** 配置文件缺失、格式错误、版本不支持或保存的 endpoint 已不存在
- **THEN** 忽略无效配置，保持停止状态，显示可操作的重新配置原因，不回退到 Windows 默认设备

#### Scenario: 用户重新配置

- **WHEN** 用户明确保存新的设备选择
- **THEN** 原配置不再用于新的启动，新的配置经过完整 preflight 后成为后续 run 的候选配置

### Requirement: 启动前安全校验

产品 SHALL 在任何音频资源打开前校验 endpoint identity、data-flow role、活动状态、VB-CABLE pair 关系和反馈风险；校验失败时 SHALL fail closed，不选择默认端点、物理扬声器、原始麦克风或其他虚拟线缆。

#### Scenario: 配置通过校验

- **WHEN** 保存的精确 ID 解析为活动且角色正确的物理输入和 VB-CABLE pair
- **THEN** 允许用户选择 AEC 或 bypass 启动，并报告本次 run 使用的精确身份

#### Scenario: 配置触发反馈风险

- **WHEN** 物理麦克风等于 `CABLE Output`，或 AEC render 等于 `CABLE Input`
- **THEN** 拒绝保存为可启动配置或拒绝启动，并报告对应的反馈风险

#### Scenario: VB-CABLE pair 缺失或歧义

- **WHEN** `CABLE Input`、`CABLE Output` 缺失、角色错误、无法配对或存在未明确选择的多个候选
- **THEN** 拒绝启动，说明需要用户处理的候选或前置条件，不下载、安装、更新或移除 VB-CABLE

### Requirement: 显式的托盘生命周期控制

系统 SHALL 通过无窗口托盘提供显式的启动 AEC、启动 bypass、停止、重启和重新配置操作；切换模式或重新配置不得隐式发送旧 run 的 PCM。

#### Scenario: 启动 AEC

- **WHEN** 用户在通过 preflight 的停止状态配置上选择启动 AEC
- **THEN** 创建新的 run、同步、AEC 和 output session，并将处理结果写入 `CABLE Input` 供下游从 `CABLE Output` 消费

#### Scenario: 启动 bypass

- **WHEN** 用户在通过 preflight 的停止状态配置上选择启动 bypass
- **THEN** 创建新的 bypass run 和 output session，不打开物理 render-loopback，不调用 AEC，并明确显示 bypass

#### Scenario: 停止或重启

- **WHEN** 用户选择停止或重启正在运行、降级或失败的 run
- **THEN** 停止提交、关闭当前资源、清空用户态 PCM 状态；重启只在新的 run 和 output session 中恢复，不重放旧 PCM

#### Scenario: 配置期间运行仍在继续

- **WHEN** 用户尝试修改设备配置或模式而当前 run 尚未停止
- **THEN** 拒绝保存对当前 run 生效的修改，或先要求用户显式停止，不静默切换 endpoint

### Requirement: 可操作的托盘状态和诊断

产品 SHALL 在托盘状态中区分未配置、已配置但停止、启动中、运行 AEC、运行 bypass、AEC 降级和失败，并为配置或运行失败提供不含 PCM 的可操作原因。

#### Scenario: 未配置状态

- **WHEN** 没有可用的持久化配置或配置尚未通过 preflight
- **THEN** 托盘显示未配置，提供重新配置入口，不显示运行中或健康状态

#### Scenario: 运行状态

- **WHEN** AEC、bypass 或 degraded run 正在运行
- **THEN** 托盘显示对应状态、当前模式和必要的输出/失败 metadata，不访问或展示 PCM

#### Scenario: 设备失效

- **WHEN** 已选物理输入或 VB-CABLE endpoint 在运行中失效
- **THEN** 当前 run 进入失败状态，托盘说明失效角色和显式 restart 要求，不自动换端点或回退输出

### Requirement: 开发和诊断覆盖

开发和诊断命令 SHALL 允许用环境变量或显式参数临时提供精确 endpoint ID，以覆盖用户态持久化配置；覆盖值不得未经用户明确保存而写回配置文件，且仍须执行相同的安全校验。

#### Scenario: 使用开发覆盖启动

- **WHEN** 开发命令提供完整的精确 endpoint 覆盖值
- **THEN** 使用覆盖值执行 preflight 和 run，不修改持久化配置，不降低端点或 VB-CABLE 校验要求

#### Scenario: 覆盖值不完整

- **WHEN** 只提供部分覆盖值或覆盖值为空、失效、角色错误
- **THEN** 命令失败并报告缺少或无效的角色，不静默混用默认设备和持久化配置
