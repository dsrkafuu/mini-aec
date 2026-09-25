# Windows 发布与开发说明

## 范围

MiniAEC 的目标平台是 Windows 11 x64。仓库中的 VB-CABLE 安装包只用于本地调试或本地构建后的手动安装，不进入 MiniAEC release。发布流程不安装、更新、移除或授权 VB-CABLE，也不请求系统重启。

应用发布包只包含 MiniAEC 可执行文件、Tauri NSIS 安装器、双语 README、发布清单和 SHA-256 校验文件。`target/`、`artifacts/`、调试符号、INF/SYS/CAT 以及 VB-CABLE 驱动材料都必须被排除。

## 本地构建

通过个人 mise 配置提供 Rust（Windows x64 MSVC）、Meson 和 Ninja；Visual Studio Build Tools 需包含 C++ 桌面开发工具和 LLVM/Clang（含 x64 libclang）。仓库的 `.tools\cargo-webrtc.cmd` 会初始化 VS 环境并设置 `LIBCLANG_PATH`。在仓库根目录执行：

```powershell
mise exec -- cargo fmt --all -- --check
mise exec -- .\.tools\cargo-webrtc.cmd test --workspace
mise exec -- .\.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
mise exec -- .\.tools\cargo-webrtc.cmd build --release
```

图标唯一设计源是 `assets/speakerphone.svg`。生成 Windows ICO 资源：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\generate-icon.ps1
```

脚本把 SVG 固定渲染为透明背景、黑色前景的 16/24/32/48/64/128/256 多尺寸 `src-tauri/icons/icon.ico`，并额外生成裁剪留白后的 32x32 `src-tauri/icons/tray-icon.png`。Tauri 应用资源使用 ICO，托盘使用专用 PNG，以避免托盘缩放后过小或模糊。

## 生成发布暂存目录

生成 Tauri NSIS 安装器后，使用发布脚本：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\package-windows-release.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\verify-windows-release.ps1 -PackageRoot .\dist\windows-x64\0.1.0
```

脚本会校验 workspace、`src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json` 的版本一致，记录 Windows x64、源 revision、构建工具链和文件清单，并生成 `release-manifest.json` 与 `SHA256SUMS.txt`。没有生产签名证书时，清单明确记录 `unsigned`，不会伪造签名状态，也不会读取或保存私钥。

如果 NSIS 输出不在默认目录，可以显式传入：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\package-windows-release.ps1 -InstallerPath .\path\to\MiniAEC_0.1.0_x64-setup.exe
```

当前项目没有内置 Tauri CLI 或生产签名凭据。NSIS 生成需要本机已经准备好 Tauri CLI 及其 Windows 构建环境；签名仍由发布者在包外部完成。

## 发布前检查

- 确认 `MiniAEC.exe` 和 NSIS 安装器都来自当前 Windows x64 release build。
- 确认 `release-manifest.json` 中版本、源 revision、签名状态和文件哈希可复核。
- 确认 `verify-windows-release.ps1` 通过，且暂存目录没有 `target/`、`artifacts/`、调试符号、驱动文件或 VB-CABLE 安装材料。
- 用户手动启动包后，再由用户确认托盘图标和已有的 `CABLE Input -> CABLE Output` 路径；MiniAEC 不启动第三方测试客户端，不安装 VB-CABLE，也不重启系统。
