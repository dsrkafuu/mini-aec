# Proposal

## Why

MiniAEC 的托盘配置、VB-CABLE 实时路径和运行时验证已经完成，但仓库还没有经过明确边界审查的 Windows 11 x64 发布包。现在需要把已验证的程序整理成可交付、可复核的包，并使用仓库中的 `assets/speakerphone.svg` 作为产品图标，避免继续沿用占位图标。

## What Changes

- 定义 MiniAEC Windows 11 x64 发布包的版本、架构、文件清单和校验信息。
- 生成不包含 `target/`、`artifacts/`、调试文件或 VB-CABLE 安装材料的发布包。
- 在发布说明中明确 VB-CABLE 由用户从官方来源自行获取、安装和管理，MiniAEC 不自动处理驱动或重启。
- 将 `assets/speakerphone.svg` 纳入应用视觉资产，更新托盘、可执行文件和安装包使用的图标，并验证发布包使用新图标。
- 重构 `README.md` 和 `README.zh.md` 为简洁的 GitHub 开源项目入口，保留产品定位、核心能力、前置条件、快速开始和文档入口，将内部技术细节下沉到 `docs/`。
- 记录当前发布包的签名状态；没有批准的生产签名时不得声称应用或驱动已正式签名。
- 保留现有 AEC M131 默认基线、托盘菜单、`CABLE Input -> CABLE Output` 路径和用户手动第三方客户端验证边界。

## Capabilities

### New Capabilities

- `windows-release-bundle`: 定义 Windows 11 x64 发布包、图标资产、双语公开 README、外部 VB-CABLE 前置条件和可复核的包内容。

### Modified Capabilities

- 无。现有实时音频、VB-CABLE 输出和托盘配置的行为合同保持不变。

## Impact

- 影响 `src-tauri/tauri.conf.json`、`src-tauri/icons/`、`assets/speakerphone.svg` 相关的图标构建配置、发布脚本或包清单，以及双语 README 发布说明。
- 可能新增版本化发布输出和 SHA-256 校验文件，但不把发布产物写入私有 `artifacts/` 录音目录。
- 不新增 Windows 音频驱动、INF/SYS/CAT、VB-CABLE 自动安装逻辑、网络服务、更新器或签名凭据。
