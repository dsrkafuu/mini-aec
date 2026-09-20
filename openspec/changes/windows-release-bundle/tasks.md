# Tasks

## 1. 发布包边界与元数据

- [ ] 1.1 定义 Windows 11 x64 发布包版本、架构、清单和来源 revision 字段，并验证 Cargo、Tauri 和包版本一致。
- [ ] 1.2 实现独立 `dist/windows-x64/<version>/` staging 与发布包白名单，排除 `target/`、`artifacts/`、调试符号、VB-CABLE 安装包和 INF/SYS/CAT，并用文件清单检查验证排除结果。
- [ ] 1.3 生成发布包清单、SHA-256 校验文件和应用签名状态，验证无生产签名凭据时明确输出 `unsigned` 且不包含证书或私钥。

## 2. 应用图标资产

- [ ] 2.1 以 `assets/speakerphone.svg` 为唯一设计源生成 Windows 兼容的多尺寸 ICO/栅格资源，验证生成资源可被 Windows/Tauri bundle 读取且保留透明背景与 speakerphone 线稿。
- [ ] 2.2 更新 Tauri 应用、托盘和 NSIS 图标引用，验证可执行文件、托盘入口和安装包入口使用同一套新图标而不再引用旧占位图标。

## 3. 发布说明与外部依赖边界

- [ ] 3.1 重构 `README.md` 和 `README.zh.md` 为一致的 GitHub 项目介绍，验证首屏、核心能力、简化音频路径、快速开始、产品边界和文档导航均清晰可读。
- [ ] 3.2 将 Rust/Tauri/WASAPI/AEC3、endpoint ID、完整诊断命令、工具链和仓库目录等内部细节下沉到专门文档，并验证双语 README 的事实、链接、VB-CABLE 前置条件、未签名状态和限制一致。
- [ ] 3.3 将发布输出目录和生成文件加入适当忽略边界，验证不删除、不暂存、不上传 `artifacts/` 私有录音。

## 4. 构建和包验证

- [ ] 4.1 运行 `cargo fmt --all -- --check`、`.tools\cargo-webrtc.cmd test --workspace` 和 `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`，确认现有音频行为未被发布改动破坏。
- [ ] 4.2 运行 Windows x64 release 构建和 Tauri NSIS bundle，验证包名、版本、架构、文件清单、排除项、图标资源和 SHA-256 全部一致。
- [ ] 4.3 由用户手动启动发布包确认托盘图标和现有 VB-CABLE 运行路径，验证代理不安装 VB-CABLE、不启动第三方客户端且不发起系统重启。
