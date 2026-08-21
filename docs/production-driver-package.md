# MiniAEC production driver package

本说明定义 `production-driver-package` 的 Windows 11 x64 构建、正式签名、可复现验证和 `production-driver-lifecycle` 交接边界；它不安装驱动、不修改证书存储或 TESTSIGNING/BCD、不改变设备或默认音频角色，也不执行重启、关机或注销。

## Stable package layout

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

`manifest.json`、`driver/MiniAECProduction.inf`、`driver/MiniAECProduction.sys`、`driver/MiniAECProduction.cat` 和 `trust/signing-evidence.json` 是 lifecycle 的固定交接路径；`trust/SysVAD-MS-PL.txt` 是随包发布的公开上游许可 notice，不包含私密材料。

## Fixed source and production identity

生产包只能来自 Microsoft `Windows-driver-samples` 的 `audio/sysvad` 路径和固定 commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89`，上游记录位于 [`driver/windows/UPSTREAM.md`](../driver/windows/UPSTREAM.md)，许可为 MS-PL，完整文本来自 pinned source 的 `LICENSE`。

生产构建要求 upstream checkout 通过 `driver/windows/scripts/verify-upstream.ps1`，并记录 `adapter.cpp`、`EndpointsCommon/minwavertstream.cpp`、`TabletAudioSample/micinwavtable.h`、`TabletAudioSample/minipairs.h` 和 `TabletAudioSample/TabletAudioSample.vcxproj` 五个项目本地 patch 的路径与 SHA-256；不从 `microsoft/audio`、滚动分支或第三方 virtual cable 导入代码。

生产 INF 使用独立的 `MiniAECProduction` hardware ID、service、binary 和 catalog identity，只声明一个 `MiniAEC Microphone` capture endpoint，保留私有 `MiniAECTransport` 控制接口，不声明 render category，并保持 protocol 1、diagnostics schema 2、48 kHz mono PCM16、每通道 480 samples 的固定 transport contract。

`validation-x64-debug`、`MiniAECValidation`、development/test certificate 和 TESTSIGNING 仅属于开发验证路径；生产 build、signing evidence 和 package preflight 都拒绝把这些 identity 带入 production package。

## Build and unsigned preflight

在仓库根目录准备可发布 runtime `.exe`，然后执行以下命令；脚本会重新检查 pinned source、runtime access policy、Windows SDK/WDK、x64 MSBuild，并只清理 `driver/windows` 下的生成目录：

```powershell
driver\windows\scripts\build-production.ps1 -RuntimeExecutable <path-to-mini-aec.exe> -CheckoutRoot .tools\sysvad-upstream
driver\windows\scripts\verify-production-package.ps1 -PackageRoot driver\windows\out\production-x64-release -CheckoutRoot .tools\sysvad-upstream -AllowUnsigned
```

构建使用 Release/x64、matching SDK/WDK、`SignMode=Off`、production INF property、固定 `DriverVer` metadata 和 MSVC `/Brepro`，先从最终 INF/SYS bytes 生成 Windows 11 x64 catalog，再把 runtime、公开 license notice 和 metadata evidence 放入独立的 ignored output tree；unsigned candidate 不能作为可分发包，也不能通过最终 Rust preflight。

## External production signing

正式签名必须使用外部批准的 production signing route；仓库脚本只接收 certificate store 中的 signer thumbprint reference 和可选 trusted timestamp URL，绝不接收、复制或保存 `.pfx`、`.p12`、`.pvk`、`.key`、`.snk` 或其他 private key container：

```powershell
driver\windows\scripts\sign-production-package.ps1 -PackageRoot driver\windows\out\production-x64-release -CheckoutRoot .tools\sysvad-upstream -SignerThumbprint <approved-public-signer-thumbprint> -TimestampUrl <approved-timestamp-url> -ReplayPackageRoot <independent-production-release>
```

签名边界对最终 catalog 执行 SHA-256 production signing，随后只读验证 CAT trust、INF catalog coverage、SYS catalog coverage、public signer chain 和 replay digest；若批准的 route 还要求 embedded SYS signature，必须由该 route 在 catalog 生成前完成并在 evidence 中声明，验证器会检查它，仓库不会把 private signing operation 扩展成证书安装或系统变更。

`verify-production-package.ps1` 默认是只读的；`-AllowUnsigned` 只用于检查 unsigned candidate，`-RequireProductionSignature` 用于检查正式签名包，`-ReportPath` 仅把计算出的公开 report 写到 package root 之外。`sign-production-package.ps1` 在验证通过后才显式更新 package 内的公开 manifest/evidence，并再次执行只读验证。

## Canonical replay and evidence

`trust/signing-evidence.json` 记录 pinned source、imported-file set、local patch hashes、Windows/Visual Studio/MSBuild/MSVC/SDK/WDK/Inf2Cat/SignTool versions、build flags、commands、package file hashes and sizes、CAT member coverage、signer subject/thumbprint、public chain、timestamp evidence、compatibility and privacy results。

Canonical payload digest 只排序并记录最终 `driver/MiniAECProduction.inf` 与 `driver/MiniAECProduction.sys` 的 UTF-8 relative path、lowercase SHA-256 和 byte size，格式为 `path NUL hash NUL size newline`；catalog-member digest 只记录 INF/SYS member path 与 hash。CAT detached signature bytes、timestamp、countersignature 和 certificate encoding 不属于 payload replay bytes，但会作为 trust metadata 记录。

Package evidence 还覆盖 `manifest.json`、runtime、INF、SYS、CAT 和 `trust/SysVAD-MS-PL.txt` 的实际 hash/size，禁止 `artifacts/`、PCM/audio、private signing suffix 和 machine-secret material。最终 Rust preflight 要求 evidence schema 1、production route、CAT/INF/SYS trust、clean replay、safe relative paths、fixed transport identity 和 `verification.read_only=true` 全部通过。

正式候选的最终只读检查命令是：

```powershell
driver\windows\scripts\verify-production-package.ps1 -PackageRoot <signed-production-release> -CheckoutRoot .tools\sysvad-upstream -ExpectedSignerThumbprint <approved-public-signer-thumbprint> -RequireProductionSignature -ReplayPackageRoot <independent-production-release>
.tools\cargo-webrtc.cmd run -p mini-aec-release -- preflight --package <signed-production-release>
```

`production-driver-lifecycle` 只消费通过 package preflight 的 manifest 和公开 evidence，再单独决定 authorization、staging、activation、upgrade、rollback 或 uninstall；package verification 只能报告 package readiness，不能报告 endpoint activation 或安装成功。任何所需 Windows restart 都由用户手动完成。
