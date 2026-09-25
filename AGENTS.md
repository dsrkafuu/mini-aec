# MiniAEC 开发指南

## 修改前必读

修改音频采集、同步、AEC、依赖、VB-CABLE 输出或验证代码前，先阅读：

1. `docs/technical-plan.md`
2. `docs/aec-baseline.md`
3. `vendor/UPSTREAM.md`
4. `docs/upstream-upgrade-plan.md`
5. `openspec/config.yaml` 与当前适用的 OpenSpec 技能文件

## 产品边界

- 产品名为 `MiniAEC`，Rust 和 Cargo 标识使用 `mini-aec`。
- 支持平台为 Windows 11 x64。
- 应用是无窗口的 Tauri 2 Rust 托盘宿主，不添加 WebView、React、TypeScript、Vite、Bun 或设置前端，除非用户明确批准。
- MiniAEC 只做声学回声消除，不在信号链中加入降噪、自动增益、均衡、去混响或语音增强。
- 输出依赖是用户单独安装的 VB-CABLE：MiniAEC 写入 `CABLE Input`，录音和会议客户端读取 `CABLE Output`。
- 仓库可保留用户提供的官方 VB-CABLE 安装包，用于本地调试或本地构建后的手动安装；安装包不进入 MiniAEC release。
- MiniAEC 产品代码不自动下载、安装、更新、卸载、授权或改名 VB-CABLE，用户自行安装和管理。
- MiniAEC 不拥有或发布 Windows 音频驱动、INF/SYS/CAT、驱动签名或驱动安装器；产品代码是用户态 Rust。
- 托盘壳只能算工程基线，不等于可发布产品。

## 当前项目状态

- React/Bun/WebView 脚手架已移除。
- Tauri 托盘宿主可编译且不创建应用窗口。
- `mini-aec-lab` 保留双 WASAPI 采集和基于 QPC 对齐的离线 AEC。
- 当前 AEC 基线是 FreeDesktop `webrtc-audio-processing 2.1`，算法基于 WebRTC M131，Rust 依赖为 `webrtc-audio-processing 2.1.0`。
- 当前配置使用上游 AEC3 默认值；旧的抑制调参、盲测和线性输出诊断实验已退出产品路线。
- SysVAD 只保留在 Git 历史中，不是当前产品或发布路径。

## AEC 依赖规则

- M131 快照保持冻结，除非用户明确批准升级变更。
- 普通功能开发不得跟踪或复制 Google WebRTC `main`。
- 不得直接升级 Rust wrapper、FreeDesktop 快照或 vendored 构建层；升级必须遵循 `docs/upstream-upgrade-plan.md`。
- 尽量不要修改 `webrtc/modules/audio_processing/aec3/`；每个本地 vendor 修改都要记录到 `vendor/UPSTREAM.md`。
- 产品代码必须依赖项目自有且可替换的 `EchoCanceller` 边界，WebRTC 类型只留在 adapter 内部。
- 在实时 `CABLE Output` 路径稳定且有相同输入证据证明默认基线存在具体问题前，不重新引入产品级 AEC 调参 profile。

## VB-CABLE 规则

- 解析精确 Windows endpoint ID，校验 render/capture 角色，并用设备元数据交叉确认厂商身份；友好名称只能用于诊断。
- 拒绝把 `CABLE Output` 当物理麦克风，也拒绝把 `CABLE Input` 当物理 render-loopback 参考。
- 不得静默回退到 Windows 默认端点、物理扬声器、原始麦克风或其他虚拟线缆。
- VB-CABLE 的安装、移除、授权和系统重启都是用户在 MiniAEC 外部手动完成的操作。

## 系统重启安全

- 禁止发起、计划或调用系统重启、关机或注销；需要时说明原因并停止，让用户保存工作后自行操作。
- 用户此前对安装、回滚或验证的授权不包含代理发起重启；任何重启都只能由用户执行。

## 验证与隐私

- 算法变更必须用相同输入比较旧版和新版，至少比较远端回声、收敛、双讲语音保留、运行时间和失败行为；不能只比较抑制量。
- 端到端验证必须是写入 `CABLE Input`、再由配对的 `CABLE Output` 消费，而不是只检查 WAV 文件。
- Windows Recorder、Discord、会议软件和其他第三方测试客户端由用户手动启动、配置、开始、停止和关闭；代理只运行 MiniAEC 命令并读取元数据，不得启动或操作这些客户端。
- 自动检查不得下载、安装、更新或移除 VB-CABLE，不得修改驱动、证书、启动配置、设备或 Windows 默认音频角色。
- `artifacts/` 是私有本地录音和证据目录；除非用户明确请求删除，否则不得暂存、提交、上传或删除其中内容。
- 只能提交 `testdata/` 下有来源和许可证记录的可再分发合成或公开素材。

## 修改范围

- 不改写生成的 OpenSpec 技能或第三方 vendored 文档。

## 检查命令

- Rust、Meson、Ninja 通过用户的 mise 环境提供；Visual Studio Build Tools 提供 x64 C++ 工具链和 LLVM/Clang（含 libclang）。仓库脚本初始化 VS 并设置 `LIBCLANG_PATH`。
- 格式：`mise exec -- cargo fmt --all -- --check`
- 测试：`mise exec -- .tools\cargo-webrtc.cmd test --workspace`
- Lint：`mise exec -- .tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`
- 构建：`mise exec -- .tools\cargo-webrtc.cmd build --release`；详见 `docs/windows-release.md`。
- 项目没有前端检查。
