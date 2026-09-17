# MiniAEC production driver lifecycle

状态：`production-driver-lifecycle` 已定义生产发布合同，并实现纯 Rust manifest、包布局、兼容性、可注入生命周期协调边界、状态和 inventory 对比模型；生产证书、具体 Windows backend、真实 Windows 安装/升级/回滚/卸载验收仍是外部前置条件，不由仓库测试或 agent 自动执行。

## Scope

本合同把已通过 M1-M4 开发期验收的 Rust/Tauri runtime 和 `MiniAEC Microphone` driver 作为一个可验证的 Windows 11 x64 发布单元。

本 change 不改变 M131 AEC3、M3 QPC synchronizer、48 kHz mono PCM16/10 ms transport、driver ring、`MiniAEC Microphone` 名称、普通用户 runtime 合同或开发期 `driver-development-lifecycle`。

M5 的初始威胁模型是可信的单用户 Windows 桌面；现有 Interactive Users producer ACL、单 owner sender slot、明确 busy、session identity 和 stale-session flush 继续使用，但不宣称 per-executable trust。更强的本地进程隔离或 service broker 需要独立 security change。

## Release unit

一个 production release 包含 runtime、driver package、`manifest.json` 和公开的 signing evidence；private signing key、`.pfx`、`.p12`、`.pvk`、`.key` 等私密材料永远不进入包或仓库。

推荐布局如下；具体 installer 技术可以变化，但 manifest 和目录语义不能变化：

```text
production-release/
├─ manifest.json
├─ runtime/
│  └─ mini-aec.exe
├─ driver/
│  ├─ MiniAECProduction.inf
│  ├─ MiniAECProduction.sys
│  └─ MiniAECProduction.cat
└─ trust/
   ├─ signing-evidence.json
   └─ SysVAD-MS-PL.txt
```

`manifest.json` 至少声明 product/runtime/driver identity、release version、Windows 11 x64 target、`MiniAEC Microphone` endpoint、`MiniAECTransport` producer interface、protocol version、diagnostics schema、PCM contract、runtime/driver compatibility range、包内相对路径和 production trust evidence。

当前 driver/windows 中的 `validation-x64-debug` 包、test certificate、TESTSIGNING 和 `MiniAECValidation` identity 都是 development-only，不能填充 production release 目录或被 production preflight 接受。

## Non-mutating preflight

预检只读取包内容，不安装 driver、不写 certificate store、不改变 BCD、device、default roles 或 service；生产包构建、正式签名和 replay 规则见 [`docs/production-driver-package.md`](production-driver-package.md)：

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-release -- preflight --package <production-release-directory>
```

该 Rust preflight 只接受已完成正式签名和独立 replay 的 candidate；`verify-production-package.ps1 -AllowUnsigned` 只用于构建阶段的 unsigned 结构检查，不能替代最终 preflight。预检 fail closed，至少检查：manifest/evidence schema、product identity、Windows target、endpoint/protocol/PCM contract、runtime/driver versions、compatibility ranges、pinned SysVAD provenance、canonical INF/SYS digests、CAT member coverage、production trust chain、development/test-signing 标记、包内相对路径和必需文件，以及私密 signing-material、audio 和 `artifacts/` content。

package verifier 的通过结果是 metadata-only handoff；它不代表 endpoint activation、driver installation、default-input selection 或 lifecycle `Verified` 状态。只有 `production-driver-lifecycle` 的独立 authorization 和 staging/activation 流程才能进行系统变更。

## Lifecycle states

```text
New -> Preflighted -> Authorized -> Staged -> Activated -> Verified
                              ├-> AwaitingUserRestart -> Staged
                              └-> UninstallStaged -> Uninstalled
Staged/Activated/UninstallStaged -> RecoveryRequired -> RolledBack
```

`crates/mini-aec-release/src/lifecycle.rs` 中的 `LifecycleCoordinator` 负责把 package preflight、显式授权、staging、activation、postcondition verification、upgrade compatibility、rollback 和 uninstall 串成低频状态边界；`LifecycleBackend` 是真正 elevated installer 或 maintenance boundary 的唯一注入点。

每次 backend mutation 前 coordinator 要求 backend 声明 elevated authority，并在 staging、activation、restart observation、uninstall 或 rollback 后读取 metadata-only inventory；candidate package 还会从 package root 重新执行只读 preflight，避免使用已变化的旧内存 evidence。

inventory 包含 product/runtime/driver identity、public endpoint、producer interface、service/package identity、production trust signer、TESTSIGNING 状态、default-input roles、unrelated endpoints 和 active session identity；rollback/uninstall 目标会清空旧 session，避免旧 PCM 跨 release 复用。

Installer or maintenance boundary 负责 package registration、driver/service changes、production trust prerequisites 和 removal；tray/runtime 只负责 ordinary-user start、stop、restart 和 PCM processing。任何 restart boundary 都必须报告 pending 状态并等待用户手动重启，不能由 agent、runtime 或脚本调用 restart、shutdown 或 sign-out。

## Upgrade and rollback

升级前保存 product/package identity、public endpoint、producer interface、service/package、trust、default-input roles 和 active-session inventory。候选包必须先通过 manifest compatibility，再激活 matching runtime/driver pair；验证 `MiniAEC Microphone` 后才能把状态标记为 `Verified`。

升级中断时不得留下 unverified mixed release；必须记录 staged identity 并进入 `RecoveryRequired`，随后恢复旧版本或报告需要用户处理的 incomplete state。

卸载或 rollback 只有在 product endpoint、producer interface、targeted package/service、old session/PCM、unrelated physical endpoint 和 default-input roles 均通过 postcondition 比对后才能宣称成功。Windows 自动改变的 default role 只记录，不自动修复；主动恢复需要单独授权。

## Evidence and acceptance

Repository tests and preflight evidence are metadata-only and do not prove audible continuity. Approved Windows 11 x64 acceptance must cover endpoint enumeration, ordinary-user runtime, Windows Recorder, at least one target meeting client, sender contention, upgrade, recovery, uninstall, rollback and unrelated endpoint preservation. Private listening recordings remain under ignored `artifacts/` and are never release artifacts.

Production release remains blocked until the approved signing route, package/installer implementation and explicit Windows acceptance are available. Development validation must continue to use [`driver/windows/README.md`](../driver/windows/README.md) and [`driver/windows/VALIDATION.md`](../driver/windows/VALIDATION.md).
