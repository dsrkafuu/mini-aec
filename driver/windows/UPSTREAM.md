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

The baseline import contains no edits to upstream files. Project-owned files outside `driver/windows/vendor/sysvad` currently provide only:

1. `driver/windows/scripts/preflight.ps1`: read-only Windows, Visual Studio, SDK, WDK, MSBuild, and SignTool discovery.
2. `driver/windows/scripts/build-baseline.ps1`: builds the retained x64 Debug projects in dependency order with `SignMode=Off` and does not install a certificate or driver.
3. `driver/windows/scripts/verify-upstream.ps1`: verifies the temporary checkout commit, exact imported-file set, and SHA-256 content equality without changing either tree.
4. `driver/windows/.gitattributes`: disables Git whitespace diagnostics only for byte-identical vendored SysVAD files because the Microsoft snapshot contains existing trailing whitespace that MiniAEC must not normalize silently.
5. `driver/windows/.gitignore`: excludes generated WDK output, driver binaries, and development signing material.

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
