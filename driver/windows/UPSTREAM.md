# SysVAD upstream provenance

This file is the source of truth for the Microsoft SysVAD source slice used by the MiniAEC virtual microphone validation driver. Update it in the same commit as any upstream pin, imported-file set, license, or local patch change.

## Current pin

| Field | Value |
| --- | --- |
| Repository | `https://github.com/microsoft/Windows-driver-samples` |
| Upstream path | `audio/sysvad` |
| Commit | `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89` |
| Retrieved | 2026-07-18 |
| License | Microsoft Public License (MS-PL) |
| Local source root | `driver/windows/vendor/sysvad` |

The similarly named SysVAD copy under `https://github.com/microsoft/audio` is not an upstream for this source tree. Do not merge or refresh files from that repository without a separate approved OpenSpec change that replaces this pin.

## Imported file inventory

The import is a minimal buildable source slice for the stock `EndpointsCommon` static library and `TabletAudioSample` driver project. Files retain their upstream relative paths beneath `driver/windows/vendor/sysvad`:

- Repository root `LICENSE`, copied to `driver/windows/vendor/sysvad/LICENSE`.
- `audio/sysvad/README.md`.
- Every `audio/sysvad/*.cpp` and `audio/sysvad/*.h` file required by `TabletAudioSample.vcxproj` and its transitive headers.
- Every file under `audio/sysvad/EndpointsCommon/`, including its project and filter metadata.
- Every file under `audio/sysvad/TabletAudioSample/`, including its project, filter metadata, resources, INX templates, sources, and headers.

The import deliberately excludes `audio/sysvad/APO/`, `audio/sysvad/KeywordDetectorAdapter/`, `audio/sysvad/Package/`, and `audio/sysvad/sysvad.sln`. Those projects add APO, keyword, multi-package, WIL, and unrelated endpoint scope that the transport spike does not need. MiniAEC builds the two retained projects explicitly instead of modifying the upstream solution.

Before accepting an import or refresh, verify that both retained project directories are byte-identical to the pinned checkout and review any root-file difference individually:

```powershell
git diff --no-index --quiet -- .tools/sysvad-upstream/audio/sysvad/EndpointsCommon driver/windows/vendor/sysvad/EndpointsCommon
git diff --no-index --quiet -- .tools/sysvad-upstream/audio/sysvad/TabletAudioSample driver/windows/vendor/sysvad/TabletAudioSample
```

## License and notices

The complete upstream MS-PL text is retained at `driver/windows/vendor/sysvad/LICENSE`. Copyright, patent, trademark, and attribution notices present in imported source files must remain intact. Source redistribution must include the complete MS-PL text; compiled distribution must use a license that complies with MS-PL.

No imported source, license, or notice file comes from `microsoft/audio`, a third-party virtual audio project, or an unpinned branch.

## Local patch ledger

The original baseline import was byte-identical to upstream. The active validation transport now contains the following recorded changes:

1. `driver/windows/scripts/preflight.ps1`: read-only Windows, Visual Studio, complete SDK, WDK, x64 MSBuild, Spectre library, and SignTool discovery, including enforcement of a matching SDK/WDK build number.
2. `driver/windows/scripts/build-baseline.ps1`: explicitly selects the matching SDK/WDK version and builds the retained x64 Debug projects in dependency order with the x64 MSBuild host and `SignMode=Off`; it does not install a certificate or driver.
3. `driver/windows/scripts/verify-upstream.ps1`: verifies the temporary checkout commit, exact imported-file set, and SHA-256 content equality without changing either tree.
4. `driver/windows/.gitattributes`: disables Git whitespace diagnostics only for byte-identical vendored SysVAD files because the Microsoft snapshot contains existing trailing whitespace that MiniAEC must not normalize silently.
5. `driver/windows/.gitignore`: excludes generated WDK output, driver binaries, and development signing material.
6. `driver/windows/mini-aec/MiniAecProtocol.h`, `MiniAecSecurity.h`, `MiniAecTransport.h`, `MiniAecTransport.cpp`, `MiniAecWaveTable.h`, and `MiniAECValidation.inx`: project-owned fixed-frame protocol, protected control-device DACL with full control for SYSTEM/Administrators and read/write for Interactive Users, explicit single-owner busy arbitration, 10-frame driver-owned ring, capture-clock reader, 48 kHz mono PCM16 native format, diagnostics schema, and one-endpoint development INF. The normal-user access change preserves the IOCTL and diagnostics layouts while moving exclusivity from the I/O manager flag to the existing spin-lock-protected create dispatch.
7. `driver/windows/vendor/sysvad/adapter.cpp`: initializes and shuts down the project-owned control transport around PortCls, preserves the original PortCls dispatch functions for non-control devices, and skips the deliberate null render miniport entry so the driver installs no render endpoint; validates OpenSpec tasks 3.1 and 3.2.
8. `driver/windows/vendor/sysvad/EndpointsCommon/minwavertstream.cpp`: replaces the sample capture tone generator at the WaveRT audio-clock write point with bounded reads from the project-owned transport; validates OpenSpec task 3.6.
9. `driver/windows/vendor/sysvad/TabletAudioSample/minipairs.h` and `micinwavtable.h`: retain only the MicIn capture miniport, select the project-owned 48 kHz mono PCM16 format table, and limit the validation miniport to one capture stream; validates OpenSpec tasks 3.1 and 3.3.
10. `driver/windows/vendor/sysvad/TabletAudioSample/TabletAudioSample.vcxproj`: compiles the project-owned transport, links `wdmsec.lib`, disables unused Bluetooth and USB sideband endpoint initialization, selects the project-owned validation or production INF through the `MiniAecInf` property while retaining the validation INF as the default, and provides a production-only fixed `DriverVer` metadata path for reproducible builds; validates the package build boundary.
11. `driver/windows/scripts/verify-upstream.ps1`: continues to require the exact imported file set and byte equality for every unmodified file while allowing only the five vendored paths listed above to differ from the pinned commit.
12. `driver/windows/scripts/verify-runtime-access-policy.ps1`, `runtime-access-validation.ps1`, and `build-validation.ps1`: verify the exact project-owned access policy, fixed protocol, capture-only endpoint and explicit busy path before building, and provide a metadata-only non-elevated runtime harness that cannot sign, install, restart or remove a driver and cannot alter boot or default audio roles; validates OpenSpec tasks 1.5, 3.1 through 3.5, and 4.6.

Every future edit beneath `driver/windows/vendor/sysvad` must be added here with the affected paths, purpose, behavioral impact, and a link to the validating OpenSpec task. Build adaptations should remain in project-owned scripts or project files when possible.

## Build baseline

Run the read-only prerequisite report first:

```powershell
./driver/windows/scripts/preflight.ps1
```

When all prerequisites are present, build the unsigned stock x64 Debug slice without installing anything:

```powershell
./driver/windows/scripts/build-baseline.ps1
```

The exact environment and observed result are recorded in `driver/windows/BASELINE.md`.

To audit the import against a checkout of the pinned repository:

```powershell
./driver/windows/scripts/verify-upstream.ps1 -CheckoutRoot ./.tools/sysvad-upstream
```

## Update policy

This snapshot is frozen for the current virtual microphone transport change. Do not follow `main` automatically. Any future upgrade must pin an immutable commit, compare the complete imported file set and license, reapply each ledger entry explicitly, build from clean prerequisites, and repeat the driver lifecycle acceptance scenarios.
