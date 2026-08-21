## Context

See proposal.md for the motivation. MiniAEC currently has a pinned Microsoft SysVAD source slice, a development-only `validation-x64-debug` build and lifecycle, and a Rust `mini-aec-release` crate that validates the production manifest, required file paths, private-signing suffixes, compatibility, and lifecycle state without touching Windows. `docs/production-driver-lifecycle.md` already fixes the production release layout, `MiniAECProduction` filenames, public endpoint identity, transport contract, trust boundary, and non-mutating preflight boundary.

The package design must preserve the fixed SysVAD commit and local patch ledger in `driver/windows/UPSTREAM.md`, the existing `MiniAEC Microphone` capture-only endpoint, the 48 kHz mono PCM16/10 ms protocol, the Interactive Users runtime path, and the rule that package work never installs a driver or initiates an operating-system restart. It must also keep the frozen WebRTC M131 AEC baseline and real-time audio workers outside the package build path.

## Goals / Non-Goals

**Goals:**

- Produce a distinct production driver payload and release handoff containing `MiniAECProduction.inf`, `MiniAECProduction.sys`, `MiniAECProduction.cat`, the existing release manifest, and public signing/build evidence.
- Make the exact SysVAD source, project-owned patch set, Windows x64 toolchain, package inputs, content hashes, catalog coverage, and trust results reviewable and replayable.
- Extend the existing non-mutating release preflight so package verification can fail closed on development identities, missing catalog coverage, invalid trust, incompatible manifest data, non-reproducible inputs, or private material.
- Keep package generation, signing, and verification separate from the lifecycle authority that later performs explicitly authorized install, activation, upgrade, rollback, uninstall, and restart-boundary handling.

**Non-Goals:**

- Installing, updating, activating, restarting, uninstalling, or rolling back any Windows driver, device, service, certificate, boot setting, or default-audio role.
- Choosing a new SysVAD commit, importing from `microsoft/audio`, changing the WDK source slice, or changing the fixed transport, endpoint, runtime, AEC, or synchronization contracts.
- Storing a production private key, certificate private container, signing secret, private recording, PCM, or meeting content in the repository or release output.
- Selecting a specific external certificate provider when the approved production signing route can remain a release input; the package contract records and verifies the selected route and public trust evidence.

## Decisions

### 1. Keep the pinned SysVAD checkout and patch ledger as the package source of truth

The production build starts by running the existing upstream verification against the exact commit and imported-file set, then records a source-tree digest and the local patch ledger in package evidence. The production package is built from the project-owned source slice rather than silently refreshing upstream or copying from another SysVAD repository. A source or patch mismatch is a hard preflight failure.

This preserves the current auditability rule and avoids treating a package build as an implicit upstream upgrade. A rolling Git checkout was rejected because it would make a signed driver package unreproducible and would bypass `docs/upstream-upgrade-plan.md`.

### 2. Create a separate production identity and output tree

Production output uses the existing lifecycle layout under a production release root: `manifest.json`, `runtime/`, `driver/MiniAECProduction.inf`, `driver/MiniAECProduction.sys`, `driver/MiniAECProduction.cat`, `trust/signing-evidence.json`, and the public `trust/SysVAD-MS-PL.txt` notice. Build intermediates and unsigned staging files remain in an ignored production output directory separate from `driver/windows/out/validation-x64-debug/`.

The production INF, hardware ID, service name, catalog name, and package metadata are project-owned production identities. The retained SysVAD code continues to publish only the one capture endpoint and private control interface. The development `MiniAECValidation` identity remains available only to the development lifecycle and is rejected by production preflight.

Reusing the validation INF and renaming the final file was rejected because the package would retain development identity in the INF, service, hardware ID, or catalog metadata and would make accidental test-signing promotion possible.

### 3. Extend the existing release preflight instead of adding a second lifecycle authority

The `mini-aec-release` crate remains the pure package and lifecycle-contract boundary. Its package preflight will continue to parse the existing manifest and safe relative paths, and will additionally load the public evidence referenced by `trust/signing-evidence.json` to verify package digests, source/build provenance, catalog coverage, signing results, and production separation. The command will report structured failures and will not call installation, certificate-store, BCD, device, default-role, restart, or sign-out operations.

The release manifest remains the lifecycle identity source of truth. Package-specific details that are too granular for the existing manifest are carried in the referenced public evidence document, which avoids an unrelated lifecycle schema fork while allowing the package verifier to evolve its evidence schema explicitly. Any manifest or evidence schema change is versioned and rejected when unsupported.

Adding a separate installer-side manifest parser was rejected because it would permit the package builder and `production-driver-lifecycle` to disagree about endpoint, protocol, runtime/driver compatibility, or trust identity.

### 4. Separate deterministic payload verification from variable signature bytes

The build phase produces a canonical package input record, canonical INF/SYS payload digests, and a catalog member set; the production WDK project fixes DriverVer metadata and enables the MSVC `/Brepro` linker mode so two clean builds compare identical payload bytes. The signing phase consumes that staged content and produces the production CAT signature using an externally controlled approved route. Verification compares the replayed payload and catalog coverage to the canonical record, then validates the signed CAT and any required embedded SYS signature. Signature timestamps, countersignatures, and certificate encoding are recorded as trust evidence but are not treated as deterministic payload bytes.

This distinction makes reproducible verification practical while preserving normal production signing behavior. Requiring the fully signed CAT to have byte-identical output across signing runs was rejected because trusted timestamping and signature metadata can legitimately vary without changing the INF/SYS payload or catalog coverage.

### 5. Treat INF/SYS/CAT as one catalog-trusted driver package

The catalog is generated from the final packaged INF and SYS bytes and is signed only after catalog generation. The verifier checks the catalog's production trust chain, checks that the catalog member hashes match the exact package files, reports INF coverage and SYS coverage separately, and checks an embedded SYS signature when required by the approved signing route. The package cannot pass with an unsigned CAT, a development signer, a mismatched catalog member, or a package file changed after signing.

The INF is not treated as an independent Authenticode binary; its formal package trust is proven by its exact membership in the signed catalog. This reflects the Windows driver package trust model while making the user-visible INF/SYS/CAT coverage explicit in release evidence.

### 6. Keep signing secrets outside the repository and package

The signing workflow accepts a secure external signing route or signer-selected certificate reference and emits only public signer subject, thumbprint, chain metadata, route identifier, verification tool/version, and timestamp evidence. It never copies a `.pfx`, `.p12`, `.pvk`, `.key`, `.snk`, or equivalent private material into the release root, evidence directory, or committed tree. Preflight scans the complete package tree for prohibited suffixes and rejects them.

A checked-in development certificate or local test-signing flow was rejected because it would conflate `validation-x64-debug`, TESTSIGNING, and the production release contract.

### 7. Make replay and clean-environment verification explicit

The production build records Windows edition/build, Visual Studio/MSBuild, MSVC, SDK, WDK, Inf2Cat, and SignTool versions, source and patch digests, relevant build flags, and the exact commands used. A reproducibility check builds from clean output directories twice or compares an independently produced package to the recorded canonical input record. It verifies deterministic payload hashes and catalog membership before accepting a production signature.

Network access, Git metadata, private local recordings, and current machine device state are not build inputs. The clean verification path may inspect local files and cryptographic signatures, but it cannot make package or Windows state changes.

### 8. Hand the package to lifecycle only after package verification

The package verifier produces a machine-readable pass/fail report that contains package identity, trust, source, content, compatibility, and privacy results. `production-driver-lifecycle` consumes this report and the release manifest before its own authorization and staging transitions. Package readiness does not imply endpoint activation; only the separately authorized lifecycle can perform system mutation, and any restart boundary remains a pending user action.

## Risks / Trade-offs

- [The approved production signing route or matching Windows SDK/WDK prerequisites are unavailable] → Keep the candidate in a non-distributable preflight state, report the missing input, and never substitute test signing or an unverified local certificate.
- [Catalog generation or external signing changes package bytes after the canonical digest is recorded] → Generate the catalog only after final packaging, sign the final CAT, and verify every INF/SYS member hash against the final package before accepting it.
- [A reproducible build is attempted with a toolchain that emits variable PE metadata] → Record the toolchain and canonicalization rules, compare deterministic payload fields rather than detached signature timestamps, and fail when a non-approved difference remains.
- [Production and development identities drift into the same output directory] → Use separate output roots, explicit production identity checks, and a fail-closed preflight that rejects validation names, test certificates, and TESTSIGNING requirements.
- [A package verifier accidentally grows machine-changing behavior] → Keep it in the existing pure Rust preflight boundary, test command behavior against read-only guarantees, and leave all installation, service, device, certificate-store, BCD, default-role, and restart operations to `production-driver-lifecycle`.
- [A public evidence file is incomplete or contains sensitive material] → Require schema validation, package-wide private-suffix scanning, metadata-only evidence checks, and a release-blocking failure before any lifecycle handoff.

## Migration Plan

1. Extend the package and evidence schemas in the existing release contract while preserving the fixed lifecycle manifest identity and development validation workflow.
2. Add production-only driver project/INF/package generation and source/toolchain provenance capture under separate ignored output paths.
3. Build an unsigned production candidate from the pinned SysVAD snapshot, generate the catalog from final INF/SYS contents, and run clean replay verification without installing anything.
4. Supply the candidate to the approved external production signing route, then verify CAT trust, INF/SYS catalog coverage, optional embedded SYS signing, public evidence, and absence of private or development material.
5. Run repository tests and read-only package preflight from a clean environment; retain only metadata evidence in the release package and keep any private audio under user-managed ignored paths.
6. Hand a passing package and manifest to the existing `production-driver-lifecycle` change for a separately authorized staging and Windows acceptance workflow; this change stops before installation, activation, rollback, uninstall, or restart.
7. If package verification fails, discard only the unaccepted staged candidate and correct the source, build, signing, or evidence input; do not mutate the installed development package or Windows state as part of package recovery.
