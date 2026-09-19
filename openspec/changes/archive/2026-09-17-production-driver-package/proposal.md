## Why

MiniAEC has a validated SysVAD-derived `MiniAEC Microphone` transport, but the repository still produces only a development validation package and has no reproducible production artifact that proves the fixed upstream snapshot, INF/SYS/CAT coverage, or production trust chain. This change supplies the signed Windows 11 x64 driver package and reviewable verification evidence required by the existing `production-driver-lifecycle` release contract, without performing machine-changing installation work.

## What Changes

- Define a production driver package built from the pinned Microsoft Windows-driver-samples SysVAD commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89` and the repository's recorded local patch ledger.
- Define a production-only package identity and layout for the `MiniAEC Microphone` capture endpoint, fixed `MiniAECTransport` protocol, and runtime/driver compatibility metadata consumed by `production-driver-lifecycle`.
- Add a formal signing contract for the INF/SYS/CAT package: production trust evidence is external input, the catalog covers the packaged INF and SYS contents, and no test certificate, test mode, or private signing material enters the release artifact.
- Add deterministic package generation and read-only verification that checks source provenance, imported-file equality, build inputs, architecture, endpoint/category scope, package hashes, catalog coverage, signature chain, manifest compatibility, and absence of development-only identities.
- Define reproducible release evidence with tool versions, pinned inputs, commands, hashes, trust metadata, and verification results while keeping private recordings and private signing keys outside the package and repository.
- Connect the package output to the existing `production-driver-lifecycle` preflight, staging, activation, upgrade, rollback, and uninstall contract without duplicating lifecycle authority or moving installation into the runtime audio path.
- Keep this change limited to package, signing, verification, and release documentation; it does not install or update a driver, modify the certificate store or boot configuration, enable test signing, execute lifecycle mutation, or initiate restart, shutdown, or sign-out.

## Capabilities

### New Capabilities

- `production-driver-package`: Reproducible Windows 11 x64 production driver package generation, formal INF/SYS/CAT trust verification, provenance, and release evidence for the lifecycle boundary.

### Modified Capabilities

None. The existing `production-driver-lifecycle` change remains the owner of machine-changing lifecycle behavior; this capability provides the immutable package and preflight inputs it consumes.

## Impact

- Affects `driver/windows` production build and packaging scripts, release manifest/package verification, signing-evidence schemas, and documentation linking package outputs to `production-driver-lifecycle`.
- Uses the already pinned SysVAD source and existing project-owned transport; it does not upgrade SysVAD, WebRTC, the frozen M131 AEC baseline, the PCM protocol, or the real-time engine.
- Requires an externally supplied approved Windows production signing route and public certificate/chain evidence; private keys and machine-specific certificate material remain outside the repository and release package.
- Enables repository-level and clean-environment verification only. Any future install, activation, upgrade, rollback, uninstall, or restart-boundary acceptance remains separately authorized under `production-driver-lifecycle` and is not performed by this change.
