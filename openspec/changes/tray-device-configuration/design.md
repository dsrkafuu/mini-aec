# Design

## Context

See `proposal.md` for motivation. 当前托盘在 `src-tauri` 中直接从四个环境变量构造 `TrayConfiguration`，引擎和 Windows 适配器已经提供精确 endpoint 校验、VB-CABLE pair preflight、AEC/bypass 和 stop/restart 能力。设计需要把配置来源和托盘控制补齐，同时不把 Windows 音频类型或 AEC 类型泄漏到托盘边界。

## Goals / Non-Goals

**Goals:**

- 在无 WebView 的托盘菜单中提供可刷新的设备候选和精确选择。
- 以用户本地、版本化、无音频内容的 JSON 配置保存 endpoint ID 和模式。
- 让托盘显式控制 AEC、bypass、stop、restart 和重新配置，并持续展示可操作状态。
- 复用现有引擎和 Windows 输出适配器的 preflight，保证持久化配置、托盘选择和环境变量覆盖使用同一套安全校验。

**Non-Goals:**

- 不添加 WebView、设置窗口、远程服务、账号或跨平台配置同步。
- 不改变 AEC M131 默认参数、实时数据路径、VB-CABLE 安装/授权边界或 Windows 默认音频角色。
- 不实现开机自启、自动选择默认设备或在配置变更时自动启动音频。

## Decisions

### 配置模型和存储

使用项目自有的版本化配置模型，保存物理麦克风 ID、物理 render ID、VB-CABLE 播放/录音 ID 和当前模式。由 Tauri 的用户配置目录解析配置文件路径，文件使用 UTF-8 JSON；写入采用临时文件加替换，避免半写入配置被下一次启动读取。配置只包含身份、模式和版本，不包含 PCM、会议内容、设备默认角色或授权信息。

选择用户配置目录而不是仓库或 `artifacts/`，因为配置属于当前 Windows 用户且不应进入 Git、证据目录或 release。保存配置不会打开设备、写入音频或改变系统默认角色。

### 设备候选和托盘交互

新增项目自有的只读候选模型：物理 capture/render 候选由引擎的 Windows 输入适配器提供，VB-CABLE pair 候选由 Windows 输出适配器根据 endpoint ID、data-flow role 和设备元数据提供。托盘只接收脱离 WASAPI 类型的显示模型和精确 ID。

托盘使用动态子菜单展示物理麦克风、物理 render 和 VB-CABLE pair；菜单文本使用友好名称、角色和必要的厂商/格式摘要，选择结果始终保存精确 ID。刷新设备只重新枚举并替换候选，不改变当前 run；当前 run 运行时禁止保存会影响它的配置。

选择菜单而不是 WebView 或手工编辑配置文件，是为了保留无窗口托盘边界，同时让普通用户不需要复制长 endpoint ID。配置文件仍作为持久化数据，不作为主要交互界面。

### 配置来源优先级

完整的开发环境变量组作为一次性的显式覆盖，优先于持久化配置；环境变量不完整时直接报告配置错误，不与持久化配置逐项拼接。没有完整覆盖时读取版本化用户配置。覆盖值只在当前进程中有效，不能由托盘启动流程自动写回。

这样既保持现有 headless/开发命令的兼容性，也避免用户误把一次诊断设备选择永久写入产品配置。

### 生命周期和状态

将当前 AEC 勾选项改为明确的 `Start AEC` 和 `Start bypass` 操作，并补充 `Stop`、`Restart` 和 `Reconfigure`。所有启动操作都先通过同一份配置 preflight，再调用现有 `Engine`；停止和重启继续由引擎负责关闭资源、清空队列和创建新 session。

状态轮询只读取 `EngineSnapshot` 和 metadata，在托盘线程更新状态与菜单可用性；实时 worker 不增加文件 I/O、控制台 I/O 或 UI 等待。无配置、配置失效、运行降级和失败状态都使用独立的可操作文本，失败时不自动重试或换设备。

### 测试边界

配置解析、版本迁移、覆盖优先级、原子序列化、菜单状态映射和非法组合使用纯 Rust/fake inventory 测试。Windows endpoint 枚举、VB-CABLE pair 识别和真实客户端消费继续使用现有 Windows 构建与用户手动运行时验证，不在自动测试中安装、更新或移除 VB-CABLE。

## Risks / Trade-offs

- [endpoint ID 会因设备重装或驱动变化而失效] -> 启动前重新解析并报告具体角色，保持停止，不按名称或默认端点回退。
- [动态托盘菜单在设备较多时不易浏览] -> 显示角色分组和友好名称，保留刷新与重新配置入口；不把完整长 ID 作为唯一显示文本。
- [配置文件替换可能被权限或占用阻断] -> 先写同目录临时文件并完成解析校验，再替换；失败时保留旧配置并报告错误。
- [环境变量覆盖与持久化配置可能被混淆] -> 只接受完整覆盖组，状态中标记临时覆盖，且不自动写回配置。
- [托盘菜单无法表达复杂的设备诊断] -> 菜单只承担选择和低频状态；详细 metadata 继续由 headless 验证和现有证据路径提供。

## Migration Plan

1. 先加入版本化配置模型、只读候选模型和纯逻辑测试，保持现有四个环境变量路径可用。
2. 接入托盘动态候选、保存/读取和显式生命周期菜单；首次启动无配置时保持停止。
3. 在 fake inventory 和 fake engine 上验证非法选择、重启隔离、覆盖优先级及状态映射。
4. 通过格式、workspace test、严格 Clippy 和 Windows x64 构建后，由用户手动完成 VB-CABLE、Recorder 和目标客户端运行时验证。

回滚时删除用户配置文件即可恢复未配置状态；代码回滚保留现有环境变量开发路径，不触碰 Windows 设备、默认角色或 VB-CABLE 安装。
