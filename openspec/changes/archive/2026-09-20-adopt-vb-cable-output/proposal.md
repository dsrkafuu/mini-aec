# 变更提案

## 背景

MiniAEC 在可预见的未来不投入自有 Windows 生产驱动和正式驱动签名，因此不能继续把自有驱动作为可分发产品路径。产品改为依赖用户单独安装的 VB-CABLE，同时保留 AEC 范围、冻结的 WebRTC 基线和无窗口 Rust 架构。

## 变更内容

- 用 VB-CABLE 替换 `MiniAEC Microphone`：MiniAEC 写入 `CABLE Input`，目标客户端读取 `CABLE Output`。
- 从当前产品契约和路线中移除 SysVAD、INF/SYS/CAT、签名、安装、升级、回滚和卸载职责。
- 要求用户从 VB-Audio 官方来源获取和管理 VB-CABLE；仓库可保留用户提供的官方安装包供本地调试和构建后手动安装，但不进入 release，MiniAEC 产品代码不自动执行其生命周期操作。
- 增加精确 endpoint ID、角色、pair 证据、反馈风险和失败处理。
- 保留有界实时引擎、AEC-only 范围、WebRTC M131/AEC3 默认基线、metadata 诊断和用户控制的重启边界。
- 把端到端和长时验收改为普通客户端消费 `CABLE Output`，SysVAD 结果只保留历史证据。
- 清理 `production-driver-package` 和 `production-driver-lifecycle` 的旧 change archive，不把其未完成生产驱动需求同步到主规格。

## 目标

- 将 VB-CABLE 确立为 MiniAEC 唯一支持的虚拟音频输出依赖。
- 统一产品路径、端点方向和外部依赖所有权。
- 让实现和验证不再需要 MiniAEC 拥有、签名或分发内核驱动。

## 非目标

- 不由 MiniAEC 自动下载、授权、安装、更新、卸载或改名 VB-CABLE。
- 不改名 VB-CABLE，不把 `CABLE Output` 宣称为 `MiniAEC Microphone`。
- 不改变 AEC 算法，也不加入降噪、增益、均衡、去混响或语音增强。
- 不恢复自有 Windows 驱动和正式签名发布路线。

## 影响

当前产品文档、OpenSpec 主规格、Rust 输出适配器、endpoint preflight、WASAPI render、托盘状态和验证命令都切换到 VB-CABLE。用户必须在目标应用中选择 `CABLE Output`。VB-CABLE 仍由用户单独安装和管理；仓库中的安装包仅用于本地调试和构建后手动安装，不进入 release，未来任何正式分发都需要另一个许可证和分发评审。
