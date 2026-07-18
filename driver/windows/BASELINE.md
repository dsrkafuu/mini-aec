# SysVAD build baseline

## Intended clean-checkout command

```powershell
./driver/windows/scripts/preflight.ps1
./driver/windows/scripts/build-baseline.ps1
```

The build script compiles `EndpointsCommon.vcxproj` followed by `TabletAudioSample.vcxproj` as x64 Debug with `SignMode=Off`. It does not enable test mode, create or install a test certificate, install a driver, or change an audio device.

## Environment observed on 2026-07-18

| Component | Observed value |
| --- | --- |
| Windows | Windows 11 Pro for Workstations 25H2, build 26200.8875 |
| Visual Studio | Visual Studio Build Tools 2026, 18.8.12009.203 |
| MSBuild | 18.8.2, x64 host |
| MSVC x64 compiler | 14.51.36248.0 |
| x64 Spectre-mitigated C++ libraries | Installed for MSVC 14.51.36231 |
| Complete Windows SDK | 10.0.28000.2114 |
| Windows Driver Kit | 10.0.28000.1839 |
| x64 SignTool | 10.0.28000.2114 |

## Current result

Both intended clean-checkout commands completed with exit code 0. The build produced `EndpointsCommon.lib` (1,934,660 bytes), `TabletAudioSample.sys` (443,904 bytes), and the stamped `ComponentizedApoSample.inf`, `ComponentizedAudioSample.inf`, and `ComponentizedAudioSampleExtension.inf` files under ignored x64 Debug output directories. WDK INF verification and Universal API validation passed; catalog generation was skipped because the unsigned baseline has no catalog input.

The build script explicitly selects the highest complete matching SDK/WDK version so MSBuild cannot silently fall back to `10.0`. It also uses the x64 MSBuild host because WDK 28000 does not install the x86 `InfVerif.dll` required by its 32-bit package-verification task. No imported SysVAD source file was modified. No driver, certificate, boot option, or audio device state was changed.
