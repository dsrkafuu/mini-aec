# Tasks

## 1. 用户配置模型

- [ ] 1.1 定义版本化的用户设备配置模型，覆盖 AEC/bypass 模式、物理麦克风、物理 render、CABLE Input 和 CABLE Output 精确 ID，并用序列化单元测试验证完整、缺失、空值和未知版本行为。
- [ ] 1.2 实现当前用户配置目录解析、读取、临时文件替换写入和无效配置隔离，并用测试验证重启读取、半写入恢复和配置文件不包含 PCM 或会议内容。
- [ ] 1.3 保留完整环境变量组作为一次性开发覆盖，验证覆盖优先级、部分覆盖失败和覆盖值不写回持久化配置。

## 2. 设备候选与安全校验

- [ ] 2.1 建立脱离 WASAPI 类型的物理 capture/render 与 VB-CABLE pair 候选模型，复用现有 endpoint identity、data-flow role、活动状态和设备元数据校验，并用 fake inventory 测试候选映射。
- [ ] 2.2 实现 AEC、bypass、反馈风险、缺失 pair、歧义 pair 和运行中重新配置的 fail-closed 规则，并用 fake engine/output 测试非法组合不会打开资源或切换旧 run。
- [ ] 2.3 实现设备刷新和精确 ID 选择，验证友好名称仅用于展示、保存结果始终包含精确 ID，且刷新不改变当前运行状态。

## 3. 无窗口托盘控制

- [ ] 3.1 将托盘配置来源接入用户配置和开发覆盖，保留现有 headless 环境变量兼容路径，并用托盘配置测试验证未配置、已配置和无效配置状态。
- [ ] 3.2 将现有 AEC 勾选操作改为明确的 Start AEC、Start bypass、Stop、Restart 和 Reconfigure 菜单动作，验证每个动作都经过 preflight、创建新 session 或清理旧 session。
- [ ] 3.3 增加按角色分组的动态设备菜单和低频状态刷新，验证托盘能区分 stopped、starting、running AEC、running bypass、degraded 和 failed，并显示可操作错误而不展示 PCM。

## 4. 文档和兼容性

- [ ] 4.1 更新 README、技术方案和实时验证文档，说明首次配置、持久化 ID、开发覆盖、显式生命周期和无 WebView 边界，并通过文档引用审查确认不重新引入驱动安装或 VB-CABLE 自动管理。
- [ ] 4.2 保持 AEC M131 默认配置、EchoCanceller 边界、CABLE Input -> CABLE Output 路径和 VB-CABLE 安装包 release 边界不变，并通过差异审查确认没有修改无关产品契约。

## 5. 验证

- [ ] 5.1 运行配置模型、候选映射、菜单状态、生命周期隔离和环境覆盖测试，确认测试覆盖正常、失败、恢复和重启场景。
- [ ] 5.2 运行 `cargo fmt --all -- --check`、`.tools\cargo-webrtc.cmd test --workspace`、`.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings` 和 `openspec validate --all --strict --no-interactive`，记录全部通过结果。
- [ ] 5.3 完成 Windows 11 x64 构建和用户手动运行时验证：用户配置物理麦克风、物理 render、CABLE Input/CABLE Output，由用户手动操作 Recorder 或目标客户端确认 bypass、AEC、stop、restart 和失效恢复；代理只读取 MiniAEC metadata，不启动第三方客户端、不安装或修改 VB-CABLE。
