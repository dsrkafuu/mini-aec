# Proposal

## Why

MiniAEC 的实时引擎和 VB-CABLE 输出已经具备产品路线所需的核心能力，但当前托盘菜单仍按开发工具设计，首次打开时设备选项不可用，还要求用户手工选择、保存和启动。现在驱动路线已经退出，托盘应直接给出可用的系统默认设备和 VB-CABLE pair，并在用户修改选择时自动应用。

## What Changes

- 将托盘菜单收敛为状态、AEC3 开关、物理麦克风选择器、物理 render 选择器、VB-CABLE pair 选择器和退出。
- 首次启动自动选择系统默认物理输入、系统默认物理输出和第一个 VB-CABLE pair；默认以 bypass 运行，AEC3 默认关闭。
- 持久化用户选择的 endpoint ID 与 Default/direct 选择意图，并在选择变化或 AEC3 开关变化时自动停止旧 run、保存配置并应用新 run；重启时 Default 重新解析当前系统默认 endpoint；不保存 PCM 或会议内容。
- 设备缺失时置灰对应菜单并阻止音频 run 启动，状态使用全大写枚举报告当前状态。
- 延续精确 endpoint ID、data-flow role、VB-CABLE pair 和反馈风险校验；只有用户界面明确提供的系统默认选项可以解析为当前默认 endpoint，其他配置缺失、失效或歧义时 fail closed。
- 保留环境变量作为开发和诊断覆盖；不添加 WebView 或设置前端，不自动安装、更新、卸载、授权或改名 VB-CABLE。

## Capabilities

### New Capabilities

- `tray-device-configuration`: 管理用户态设备选择、持久化配置和无窗口托盘生命周期控制。

### Modified Capabilities

- 无。现有实时引擎和 VB-CABLE 规格的核心音频合同保持不变。

## Impact

- 影响 `src-tauri` 的托盘菜单、默认设备解析、配置读取、状态展示和自动生命周期控制，以及 Windows endpoint 枚举适配和相关测试。
- 可能新增用户本地配置文件，但不新增音频驱动、安装器、签名材料、网络服务或 VB-CABLE 分发内容。
- 现有 headless 命令、`EchoCanceller` 边界、AEC M131 基线和 `CABLE Input -> CABLE Output` 产品路径保持兼容。
