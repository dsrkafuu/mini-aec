# Tasks

## 1. 用户配置模型

- [x] 1.1 定义版本化的用户设备配置模型，覆盖 AEC/bypass 模式、物理麦克风、物理 render、CABLE Input 和 CABLE Output 精确 ID，并用序列化单元测试验证完整、缺失、空值和未知版本行为。
- [x] 1.2 实现当前用户配置目录解析、读取、临时文件替换写入和无效配置隔离，并用测试验证重启读取、半写入恢复和配置文件不包含 PCM 或会议内容。
- [x] 1.3 保留完整环境变量组作为一次性开发覆盖，验证覆盖优先级、部分覆盖失败和覆盖值不写回持久化配置。

## 2. 设备候选与安全校验

- [x] 2.1 增加系统默认 capture/render 候选和第一个 VB-CABLE pair 默认值，验证候选缺失时不启动音频 run。
- [x] 2.2 保持 AEC、bypass、反馈风险、缺失 pair、歧义 pair 和自动切换的 fail-closed 规则，验证新配置失败时不复用旧 PCM。
- [x] 2.3 实现无需手动刷新和保存的精确 ID 选择，验证选择变化自动持久化并应用新 run。

## 3. 无窗口托盘控制

- [x] 3.1 将托盘配置来源接入自动持久化和开发覆盖，保留现有 headless 环境变量兼容路径，并验证启动默认值和覆盖优先级。
- [x] 3.2 将托盘改为默认 bypass 自动运行、Enable AEC3 开关和设备变化自动 stop/apply/start，验证每次变化创建新 session 且不提交旧 PCM。
- [x] 3.3 实现固定顺序的 Status、Enable AEC3、Input Microphone、Output Reference、VB-CABLE Pairs 和 Quit 菜单，并验证大写状态枚举与无设备置灰。

## 4. 文档和兼容性

- [x] 4.1 更新 README、技术方案和实时验证文档，说明默认候选、自动应用、状态枚举、开发覆盖和无 WebView 边界，并通过文档引用审查确认不重新引入驱动安装或 VB-CABLE 自动管理。
- [x] 4.2 保持 AEC M131 默认配置、EchoCanceller 边界、CABLE Input -> CABLE Output 路径和 VB-CABLE 安装包 release 边界不变，并通过差异审查确认没有修改无关产品契约。

## 5. 验证

- [x] 5.1 运行默认候选、自动应用、菜单状态、生命周期隔离和环境覆盖测试，确认测试覆盖正常、失败、恢复和重启场景。
- [x] 5.2 运行 `cargo fmt --all -- --check`、`.tools\cargo-webrtc.cmd test --workspace`、`.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings` 和 `openspec validate --all --strict --no-interactive`，记录全部通过结果。
- [x] 5.3 完成 Windows 11 x64 构建和用户手动运行时验证：用户配置物理麦克风、物理 render、CABLE Input/CABLE Output，由用户手动操作 Recorder 或目标客户端确认 bypass、AEC、stop、restart 和失效恢复；代理只读取 MiniAEC metadata，不启动第三方客户端、不安装或修改 VB-CABLE。
