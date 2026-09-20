# Spec Delta

## Purpose

定义 MiniAEC 的无窗口托盘配置和自动应用行为，让已安装 VB-CABLE 的普通用户打开程序后即可使用明确的系统默认设备和线路对，并能安全切换 AEC3 与设备选择。

## ADDED Requirements

### Requirement: 托盘提供带默认值的精确设备选择

产品 SHALL 通过无 WebView 的托盘菜单提供 `Input Microphone`、`Output Reference` 和 `VB-CABLE Pairs` 三个选择器。每个选择器 SHALL 使用精确 endpoint ID 作为实际身份，并使用友好名称和必要元数据作为展示信息。

#### Scenario: 首次启动使用默认设备

- **WHEN** 用户首次启动 MiniAEC 且存在活动物理输入、活动物理 render 和有效 VB-CABLE pair
- **THEN** `Input Microphone` 和 `Output Reference` 的第一个独立选项分别为 `Default (<name>)` 系统默认输入/输出，且普通候选列表仍保留该设备的独立直接选择项；`VB-CABLE Pairs` 默认选择第一个有效 pair，并使用它们对应的精确 endpoint ID

#### Scenario: 设备候选缺失

- **WHEN** 物理输入、物理 render 或 VB-CABLE pair 的必要候选缺失
- **THEN** 对应选择器置灰，音频 run 不启动，状态显示 `OFFLINE` 或 `ERROR`，且不选择其他默认设备、原始麦克风或其他虚拟线缆

#### Scenario: 用户选择设备

- **WHEN** 用户从任一选择器选择一个候选
- **THEN** 选择器使用候选的精确 endpoint ID 自动应用配置，友好名称只用于显示，不作为持久化身份

### Requirement: 用户配置自动持久化

系统 SHALL 在当前 Windows 用户范围内保存版本化 JSON 配置，内容包含模式、物理输入/输出和 VB-CABLE pair 的精确 endpoint ID，以及物理输入/输出是否选择系统默认的选择意图；不包含 PCM、会议内容、签名材料或 VB-CABLE 安装授权信息。

#### Scenario: 自动保存设备变化

- **WHEN** 用户修改输入、输出或 VB-CABLE pair 选择且新配置通过 preflight
- **THEN** 新配置自动写入用户配置文件并成为下一次启动的候选配置，不需要用户点击保存

#### Scenario: 应用重新启动

- **WHEN** MiniAEC 重新启动且上一次配置文件仍然存在
- **THEN** 恢复 AEC3 开关状态；对直接选择恢复原精确 endpoint ID，对 `Default (...)` 选择重新解析当前系统默认 endpoint 后再自动应用，不读取 PCM 或会议内容

#### Scenario: 默认选择与直接选择分别持久化

- **WHEN** 用户分别选择 `Default (...)` 或普通设备项后退出并重新启动 MiniAEC
- **THEN** 前者继续跟随启动时的当前系统默认设备，后者继续固定到所选 endpoint；两者都不按友好名称猜测

#### Scenario: 配置文件不可读或版本不支持

- **WHEN** 配置文件缺失、格式错误、版本不支持或保存的 endpoint 已不存在
- **THEN** 保持音频 run 停止，显示 `OFFLINE` 或 `ERROR`，提供默认候选或错误原因，不按友好名称猜测或混用旧配置

### Requirement: 自动应用仍执行启动前安全校验

产品 SHALL 在打开任何音频资源前校验 endpoint identity、data-flow role、活动状态、VB-CABLE pair 关系和反馈风险。用户界面明确提供的系统默认选项 SHALL 先解析为当前精确 endpoint ID；其他缺失、失效或歧义配置 SHALL fail closed。

#### Scenario: bypass 默认运行

- **WHEN** 默认 AEC3 开关为关闭且物理麦克风与有效 VB-CABLE pair 可用
- **THEN** MiniAEC 自动创建 bypass run 和 output session，不打开物理 render-loopback，不调用 AEC，并显示 `ONLINE`

#### Scenario: 启用 AEC3

- **WHEN** 用户打开 `Enable AEC3` 且物理麦克风、物理 render 和 VB-CABLE pair 均通过 preflight
- **THEN** MiniAEC 停止旧 run、创建新的 AEC run 和 output session，并将结果写入 `CABLE Input` 供 `CABLE Output` 消费

#### Scenario: 反馈风险或 pair 无效

- **WHEN** 物理麦克风等于 `CABLE Output`、AEC render 等于 `CABLE Input`、pair 缺失、角色错误、未配对或存在歧义
- **THEN** MiniAEC 不打开新的音频资源，不回退到其他设备，并显示 `ERROR` 或 `OFFLINE` 及可操作原因

### Requirement: 设备和 AEC 修改自动切换生命周期

系统 SHALL 通过 `Enable AEC3`、三个设备选择器和 `Quit MiniAEC` 提供生命周期入口；不再要求用户手动执行 Start、Stop、Restart、Save 或 Refresh。每次有效修改 SHALL 停止旧 run、清理用户态 PCM 状态并创建新 session；无效修改不得继续提交旧 run 的 PCM。

#### Scenario: 切换设备或模式

- **WHEN** 当前 run 正在运行且用户修改 AEC3 开关或任一设备选择
- **THEN** MiniAEC 显式停止当前 run，清空旧队列和输出状态，验证新配置后创建新的 run/session；切换过程不静默复用旧 PCM

#### Scenario: 自动应用失败

- **WHEN** 新选择无法通过 preflight 或设备在切换期间失效
- **THEN** 当前 run 进入 `ERROR` 或 `OFFLINE`，不自动重试、不换端点，等待设备恢复或用户再次选择

### Requirement: 托盘状态使用大写枚举

托盘第一项 SHALL 固定显示 `Status: <STATE>`，其中 `<STATE>` SHALL 使用全大写枚举，例如 `ONLINE`、`STARTING`、`STOPPING`、`DEGRADED`、`ERROR` 和 `OFFLINE`。`Enable AEC3` SHALL 为可勾选开关且首次默认关闭；没有可用设备时相关菜单 SHALL 置灰。

#### Scenario: 健康运行状态

- **WHEN** bypass 或 AEC run 正常运行
- **THEN** 托盘显示 `Status: ONLINE`，不显示 PCM 或会议内容

#### Scenario: 运行降级或失败

- **WHEN** AEC 处于降级或 endpoint/output/session 失败
- **THEN** 托盘显示 `Status: DEGRADED` 或 `Status: ERROR`，并保留可操作的失败原因而不自动回退

### Requirement: 开发和诊断覆盖保持兼容

开发和诊断命令 SHALL 继续允许完整环境变量组临时提供精确 endpoint ID，以覆盖用户态持久化配置；覆盖值不得未经用户明确保存而写回配置文件，且仍须执行相同的安全校验。

#### Scenario: 使用开发覆盖启动

- **WHEN** 开发命令提供完整的精确 endpoint 覆盖值
- **THEN** 使用覆盖值执行 preflight 和自动应用，不修改持久化配置，不降低 endpoint 或 VB-CABLE 校验要求

#### Scenario: 覆盖值不完整

- **WHEN** 只提供部分覆盖值或覆盖值为空、失效、角色错误
- **THEN** 命令失败并显示 `ERROR` 或 `OFFLINE`，不静默混用默认设备和持久化配置
