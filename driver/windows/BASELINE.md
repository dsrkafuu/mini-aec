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
| MSBuild | 18.8.2 |
| MSVC x64 compiler | 14.51.36248.0 |
| x64 Spectre-mitigated C++ libraries | Installed for MSVC 14.51.36231 |
| Complete Windows SDK | 10.0.26100.8249 |
| Incomplete Windows SDK content | 10.0.28000.1839 ARM64 and WDK-supplied content; missing the complete SDK package, UAP metadata, and x64 UCRT libraries |
| Windows Driver Kit | 10.0.28000.1839 |
| x64 SignTool | 10.0.26100.8249 |

## Current result

The read-only preflight stops with exit code 1 because there is no matching complete SDK/WDK build pair. WinGet does not report `Microsoft.WindowsSDK.10.0.28000` as installed. The first compile attempt, before the preflight completeness check was tightened, let MSBuild select the incomplete target version `10.0` and failed because `DDK_INC_PATH` was empty. Explicitly selecting `10.0.28000.0` correctly populated `DDK_INC_PATH` but then failed with MSB8036 because the complete SDK is absent.

Install the complete Windows SDK 28000 package linked from Microsoft's WDK setup step 2, then rerun the two intended clean-checkout commands. The build script now selects the highest complete matching SDK/WDK version explicitly so MSBuild cannot silently fall back to `10.0`. OpenSpec task 1.5 remains incomplete until the unsigned build succeeds. No driver, certificate, boot option, or audio device state was changed.
