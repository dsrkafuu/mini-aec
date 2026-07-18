## Why

MiniAEC 的首个可用版本必须提供项目自带的 `MiniAEC Microphone`，但仓库目前还没有经过验证的 Windows 虚拟麦克风数据通路。先用确定性 PCM 和测试签名驱动验证这个边界，可以在接入物理麦克风与实时 AEC 之前隔离内核传输、生命周期和恢复风险。

## Goals

- 固定一个可审计的 Microsoft SysVAD 上游版本及许可证基线。
- 比较私有 WaveRT render sink 转发与受限共享环形缓冲两种数据通路，并用明确的门槛选择实现方案。
- 建立最小用户态 PCM 发送程序到 `MiniAEC Microphone` 的端到端通路。
- 通过 Windows 录音工具验证连续采集、发送进程重启、驱动重启和卸载恢复。

## Non-goals

- 不接入物理麦克风、WASAPI render loopback、实时 AEC 或任何降噪与增益处理。
- 不接入托盘控制、设置界面、会议软件兼容性矩阵或默认设备切换。
- 不申请正式代码签名，不交付生产安装包，也不把测试签名流程视为发布方案。
- 不升级或修改冻结的 WebRTC AEC3 M131 基线。

## What Changes

- 新增 `MiniAEC Microphone` 虚拟采集端点及确定性 PCM 注入的可验收行为定义。
- 新增两种候选传输方案的同条件比较、选择门槛和失败回退要求。
- 新增基于测试签名的驱动构建、安装、重启、卸载和系统恢复验证要求。
- 新增 Microsoft SysVAD 来源、精确版本、MS-PL 许可证和本地改动的追踪要求。
- 新增最小用户态 PCM 发送程序及 Windows 录音工具验证流程。

## Capabilities

### New Capabilities

- `virtual-microphone-transport`: 定义确定性 PCM 从最小用户态发送程序进入 `MiniAEC Microphone` 并被 Windows 录音工具连续采集时的格式、连续性、欠载、重连和驱动重启行为。
- `driver-development-lifecycle`: 定义 SysVAD 上游与许可证追踪、测试签名驱动的受控构建与部署、驱动重启、卸载和恢复验证行为。

### Modified Capabilities

无。

## Impact

- 新增 Windows WDK 驱动原型、最小用户态 PCM 发送程序和仅供本机验证使用的测试脚本或说明。
- 以 Microsoft `Windows-driver-samples` 仓库 `audio/sysvad` 目录为唯一 SysVAD 上游，固定到 commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89`，许可证为 Microsoft Public License（MS-PL）。
- 驱动安装、重启和卸载会改变本机 Windows 音频设备状态，执行这些验证前必须取得用户明确批准，并提供恢复步骤。
- 不影响现有离线 AEC、冻结的 WebRTC 依赖、托盘壳或私有 `artifacts/` 数据。
