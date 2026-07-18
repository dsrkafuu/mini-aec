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
| Visual Studio | Visual Studio Build Tools 2026, 18.7.11925.98 |
| MSBuild | 18.7.8 |
| MSVC x64 compiler | 14.51.36248.0 |
| x64 Spectre-mitigated C++ libraries | Missing |
| Windows SDK | 10.0.26100.0 |
| Windows Driver Kit build targets | Missing |
| x64 SignTool | Missing |

## Current result

The read-only preflight correctly stops with exit code 1 and reports the missing x64 Spectre-mitigated C++ libraries, WDK build targets, and x64 SignTool. The unsigned SysVAD compile has not run, so OpenSpec task 1.5 remains incomplete. Installing or changing the toolchain requires a separate user decision; no driver, certificate, boot option, or audio device state was changed.
