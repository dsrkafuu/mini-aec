# Design

## Context

See `proposal.md` for motivation. 当前 Tauri 配置已经启用 Windows NSIS bundle，并引用 `src-tauri/icons/icon.ico`；仓库另有用户提供的 `assets/speakerphone.svg`，但二者尚未形成经过验证的产品图标链路。VB-CABLE 安装包保留在仓库中只用于本地调试和手动安装，不能进入 MiniAEC release；`artifacts/` 继续是私有证据目录。

## Goals / Non-Goals

**Goals:**

- 以现有 Tauri Windows bundle 为基础生成可识别版本和架构的 release 包。
- 让应用、托盘和安装包入口使用同一套 `speakerphone` 视觉资产，并统一为黑色前景。
- 让包内容、来源版本、SHA-256 和签名状态可以在不安装驱动的情况下复核。
- 保持 VB-CABLE 由用户外部获取和管理，发布包只说明前置条件和产品音频路径。
- 让双语 README 成为面向 GitHub 访客的产品入口，而不是内部技术手册。

**Non-Goals:**

- 不修改实时引擎、AEC3 参数、托盘设备选择或 `CABLE Input -> CABLE Output` 信号合同。
- 不获取或配置应用生产签名证书，不生成驱动签名材料，不把未签名包描述为正式签名产品。
- 不把 VB-CABLE 安装器、INF/SYS/CAT、`target/` 或 `artifacts/` 放入发布包。
- 不实现自动更新、在线下载、自动安装/卸载 VB-CABLE 或系统重启。

## Decisions

### 发布格式和输出位置

以当前 Tauri NSIS 目标作为主要 Windows 发布入口，生成带 MiniAEC 版本和 `windows-x64` 标识的安装包；同时在独立的 release staging 目录生成清单、说明和 SHA-256 文件。staging 目录使用仓库的发布输出路径，不使用 `artifacts/`，并加入忽略规则避免把构建产物提交到源码。

选择现有 NSIS 目标而不是重新设计安装器，是因为项目已经启用 Tauri bundle，能够保持无窗口托盘宿主和现有安装入口。便携压缩包可以作为后续需求，不在本 change 额外维护两套安装语义。

### 版本和来源身份

发布流程在打包前校验 Cargo 包版本、Tauri 应用版本和发布文件名使用同一版本；清单记录目标架构、源代码 revision 或等价来源标识、构建时间字段和工具链摘要。构建时间不作为内容身份，文件清单和 SHA-256 作为实际复核依据。

### 图标转换和引用

`assets/speakerphone.svg` 是唯一设计源，保留其 speakerphone 线稿几何，不在实现中重新绘制另一个图标。由于 Windows 可执行文件、托盘和 NSIS 入口需要栅格/ICO 资源，构建前将源 SVG 固定渲染为黑色前景、透明背景的应用 ICO，并额外生成裁剪留白的高分辨率托盘 PNG。Tauri 应用和 NSIS 入口使用 ICO，托盘显式使用专用 PNG；两者仍可追溯到同一份 SVG 视觉资产，避免托盘使用应用 ICO 后缩放过小或模糊。

### 发布边界和 VB-CABLE 说明

发布包只携带 MiniAEC 运行所需的应用文件和用户说明。说明明确：用户须从 VB-Audio 官方来源自行安装和管理 VB-CABLE，MiniAEC 写入 `CABLE Input`，录音或会议客户端读取 `CABLE Output`；缺少 pair 时 MiniAEC 保持 `OFFLINE`/`ERROR` 并 fail closed。发布流程不调用驱动安装器、不修改默认音频角色、不请求系统重启。

### 双语 README 信息架构

`README.md` 和 `README.zh.md` 使用相同的公开项目结构，但分别以自然的英文和中文表达：项目一句话定位、核心能力、简化音频路径、VB-CABLE 前置条件、快速开始、明确的非目标/限制、文档导航和贡献或开发入口。README 只保留帮助访客判断“这是什么、能不能用、如何开始”的信息；Rust/Tauri/WASAPI 类型、AEC3 快照细节、endpoint ID 规则、完整诊断命令、仓库目录和工具链要求分别链接到 `docs/technical-plan.md`、`docs/aec-baseline.md`、`docs/realtime-aec-validation.md`、`docs/upstream-upgrade-plan.md` 或其他专门文档，不在介绍正文重复展开。

双语文件以同一事实清单进行审查，尤其同步 Windows 11 x64 范围、VB-CABLE 用户管理边界、未签名状态、已知限制和 release 排除项；语言版本可以有本地化措辞，但不能出现不同的产品承诺。

### 签名和校验

没有批准的生产代码签名凭据时，包清单将应用标记为 `unsigned`，并明确这不等于驱动签名或 Microsoft 认证。发布流程只做只读签名状态检查和 SHA-256 生成，不把证书、私钥或测试签名材料写入仓库或包内。未来若获得应用签名能力，应作为独立变更接入，不改变本 change 的包边界。

### 验证策略

验证分为 README 内容验证、源码/构建验证、包内容验证和用户运行时验证：README 检查双语章节、事实和链接一致；源码与 workspace 检查继续使用项目既有命令；release 构建后检查包名、版本、架构、清单、排除项、图标资源和 SHA-256；用户手动安装或启动包时仍由用户操作，代理不安装 VB-CABLE、不启动第三方客户端、不重启系统。

## Risks / Trade-offs

- [没有应用生产签名可能触发 SmartScreen 警告] → 在包清单和发布说明中明确 `unsigned`，不伪造可信状态；签名接入另立 change。
- [SVG 的 `currentColor` 在 Windows 图标转换器中表现不一致] → 使用固定渲染颜色和透明背景生成多尺寸资源，并对 exe、托盘和 NSIS 入口做一致性检查。
- [NSIS 工具链或 Tauri bundle 环境缺失] → 将工具检查作为构建前置条件，保留已通过的 `cargo build --release` 作为代码构建验证，不把未生成的包报告为成功。
- [第三方 VB-CABLE 材料误入 release] → 用清单白名单和排除项检查，明确 `vendor/VBCABLE_Driver_Pack45` 只属于本地调试材料。

## Migration Plan

1. 保留现有用户配置、VB-CABLE 安装和运行时行为，只新增发布输出和图标资源链路。
2. 以当前 release 构建生成包、清单和校验文件，完成静态内容验证。
3. 由用户在已准备 VB-CABLE 的环境中手动启动包，确认托盘图标和现有音频路径；不由发布流程安装驱动或发起重启。
4. 回滚时使用上一版 MiniAEC 可执行文件或安装包，不删除用户配置、VB-CABLE 或 `artifacts/`。
