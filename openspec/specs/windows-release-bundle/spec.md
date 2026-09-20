# windows-release-bundle Specification

## Purpose

定义 MiniAEC 面向 Windows 11 x64 的可交付发布包、应用图标和包内容验证边界，让用户可以在自行准备 VB-CABLE 后获得可复核的 MiniAEC 安装材料，而不把第三方驱动或私有开发产物带入 release。

## Requirements

### Requirement: Windows x64 发布包具有明确内容边界

MiniAEC SHALL 生成带版本和 Windows x64 标识的发布包，至少包含可运行的 MiniAEC、面向用户的启动与前置条件说明、包清单和对应的 SHA-256 校验信息；发布包 SHALL 不包含 `target/`、`artifacts/`、调试符号、私有录音、VB-CABLE 安装包或 Windows 驱动文件。

#### Scenario: 发布包内容完整

- **WHEN** 从同一份 Windows x64 release 构建产物生成发布包
- **THEN** 包含版本、架构、文件清单和校验信息，并能定位 MiniAEC 的可执行入口和用户说明

#### Scenario: 私有和第三方材料被排除

- **WHEN** 对发布包执行文件清单检查
- **THEN** 不发现 `artifacts/` 私有证据、`target/` 构建缓存、调试符号、VB-CABLE 安装器、INF/SYS/CAT 或其他 Windows 驱动材料

#### Scenario: 架构和版本可识别

- **WHEN** 用户查看发布包名称、清单或版本信息
- **THEN** 能确认该包面向 Windows 11 x64，且包版本与 MiniAEC 应用版本一致

### Requirement: 发布包保留 VB-CABLE 外部管理边界

发布包 SHALL 说明用户必须从 VB-Audio 官方来源自行获取、安装、更新、授权和移除 VB-CABLE；MiniAEC SHALL 不因发布包安装或启动而自动下载、安装、更新、卸载、授权或改名 VB-CABLE，也不得发起系统重启。

#### Scenario: 用户准备外部 VB-CABLE

- **WHEN** 用户按照发布说明准备 MiniAEC 的运行环境
- **THEN** 说明要求用户外部安装受支持的 VB-CABLE pair，并明确 MiniAEC 写入 `CABLE Input`、下游客户端读取 `CABLE Output`

#### Scenario: VB-CABLE 前置条件缺失

- **WHEN** 用户未安装受支持的 VB-CABLE pair 或 pair 无法通过身份和角色校验
- **THEN** MiniAEC 保持 `OFFLINE` 或 `ERROR` 并给出可操作原因，不从发布包中寻找替代驱动或静默回退到其他设备

### Requirement: 应用和托盘使用提供的图标资产

发布构建 SHALL 以仓库中的 `assets/speakerphone.svg` 作为 MiniAEC 的图标源，生成 Windows 可用的应用、托盘和安装包图标；发布包中的这些入口 SHALL 使用同一套新图标，不继续使用旧的占位图标。

#### Scenario: 图标资产进入发布构建

- **WHEN** 从仓库构建 Windows x64 发布包
- **THEN** 可追溯到 `assets/speakerphone.svg`，并能在可执行文件、托盘图标和安装包入口看到对应的 speakerphone 视觉标识

#### Scenario: 图标格式和尺寸可用

- **WHEN** Windows 加载应用、托盘或安装包图标
- **THEN** 图标包含构建所需的 Windows 兼容格式和尺寸，透明背景与线稿不会导致入口缺图、损坏或无法加载

### Requirement: 发布状态和校验结果可复核

发布输出 SHALL 记录构建版本、目标架构、源代码版本或等价来源标识、包文件清单、SHA-256 校验值和应用签名状态；没有批准的生产签名材料时 SHALL 明确标记为未签名，不得声称应用或驱动已经正式签名。

#### Scenario: 校验发布包

- **WHEN** 用户根据发布清单重新计算包文件的 SHA-256
- **THEN** 计算结果与发布输出一致，并能确认包内容未被替换

#### Scenario: 没有生产签名

- **WHEN** 构建环境没有批准的生产代码签名凭据
- **THEN** 发布信息标记应用为未签名，仍可用于本地或手动测试，但不伪造签名状态、不生成驱动签名材料

### Requirement: 双语 README 作为 GitHub 项目入口

项目 SHALL 维护 `README.md` 和 `README.zh.md` 两份面向 GitHub 访客的简洁项目介绍，使用一致的产品事实和主要章节，至少说明 MiniAEC 的用途、核心能力、`CABLE Input -> CABLE Output` 高层路径、VB-CABLE 前置条件、快速开始、产品边界和深入文档入口；内部实现细节 SHALL 下沉到 `docs/`，不在 README 主体展开。

#### Scenario: 首次访问可以理解产品

- **WHEN** 用户只阅读任一语言的 README 首屏和快速开始部分
- **THEN** 能理解 MiniAEC 解决的问题、需要用户准备的 VB-CABLE、基本使用方式和不包含的能力，不需要先阅读 Rust、WASAPI、AEC3 快照或 endpoint ID 细节

#### Scenario: 双语内容保持一致

- **WHEN** 用户在 `README.md` 和 `README.zh.md` 之间切换
- **THEN** 两份 README 的产品定位、功能边界、外部依赖、签名状态和文档链接表达相同事实，不出现一份语言独有的行为承诺或过期说明

#### Scenario: 技术细节链接到专门文档

- **WHEN** 用户需要了解构建、诊断、AEC 基线、实时验证或上游依赖
- **THEN** README 提供对应 `docs/` 或 `vendor/` 文档入口，长命令、内部目录、端点选择规则和工具链细节不在项目介绍中重复展开
