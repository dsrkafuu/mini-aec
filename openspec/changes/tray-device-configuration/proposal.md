# Proposal

## Why

MiniAEC 的实时引擎和 VB-CABLE 输出已经具备产品路线所需的核心能力，但无窗口托盘仍要求用户手工设置四个 endpoint 环境变量，也没有持久化配置和清晰的停止、重新配置入口。这使产品仍然偏向开发工具，且容易把物理输入、物理播放回环和 VB-CABLE pair 配错。现在驱动路线已经退出，应该先补齐用户态设备配置边界。

## What Changes

- 新增本地设备配置能力，让用户选择精确的物理麦克风、物理 render-loopback 和 VB-CABLE pair。
- 持久化用户选择的 endpoint ID，并在托盘状态中显示必要的设备元数据；不保存 PCM 或会议内容。
- 提供明确的启动 AEC、启动 bypass、停止、重启和重新配置操作，并报告可操作的失败原因。
- 延续精确 endpoint ID、data-flow role、VB-CABLE pair 和反馈风险校验；配置缺失、失效或歧义时 fail closed，不回退到默认端点。
- 保留环境变量作为开发和诊断覆盖；不添加 WebView 或设置前端，不自动安装、更新、卸载、授权或改名 VB-CABLE。

## Capabilities

### New Capabilities

- `tray-device-configuration`: 管理用户态设备选择、持久化配置和无窗口托盘生命周期控制。

### Modified Capabilities

- 无。现有实时引擎和 VB-CABLE 规格的核心音频合同保持不变。

## Impact

- 影响 `src-tauri` 的托盘菜单、配置读取、状态展示和生命周期控制，以及 Windows endpoint 枚举适配和相关测试。
- 可能新增用户本地配置文件，但不新增音频驱动、安装器、签名材料、网络服务或 VB-CABLE 分发内容。
- 现有 headless 命令、`EchoCanceller` 边界、AEC M131 基线和 `CABLE Input -> CABLE Output` 产品路径保持兼容。
