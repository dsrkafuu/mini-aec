## 1. Package contract and provenance

- [x] 1.1 Define the production package evidence schema for source commit, imported-file and patch identities, toolchain versions, build inputs, payload digests, catalog coverage, signer metadata, trust-chain results, verification commands, and lifecycle compatibility.
- [x] 1.2 Align the package evidence and manifest references with `docs/production-driver-lifecycle.md`, preserving `manifest.json`, `driver/MiniAECProduction.inf`, `driver/MiniAECProduction.sys`, `driver/MiniAECProduction.cat`, and `trust/signing-evidence.json` as the stable handoff paths.
- [x] 1.3 Document the exact pinned SysVAD source, MS-PL notice requirements, production-only identity, capture-only endpoint scope, fixed transport contract, and explicit separation from `validation-x64-debug`, `MiniAECValidation`, test certificates, and TESTSIGNING.
- [x] 1.4 Document the external production signing input, public signer and chain evidence, INF/SYS catalog-coverage semantics, optional embedded SYS-signature check, private-key boundary, and read-only package verification boundary.
- [x] 1.5 Define the canonical payload digest and catalog-member replay rules so variable CAT signature timestamps or certificate encoding are recorded as trust metadata rather than treated as reproducible payload bytes.

## 2. Production driver package build

- [x] 2.1 Add the production-only INF, hardware ID, service identity, catalog reference, and project configuration derived from the pinned SysVAD slice, exposing only `MiniAEC Microphone` capture and the private `MiniAECTransport` interface with no render category.
- [x] 2.2 Implement a production x64 build entry point that runs the existing upstream verification and prerequisite preflight, selects a matching Windows SDK/WDK and x64 MSBuild environment, and writes only to a separate ignored production output tree.
- [x] 2.3 Capture source-tree, imported-file, local-patch, toolchain, build-flag, and command provenance before producing the production INF/SYS payload, and fail before packaging when the pinned snapshot or declared prerequisites do not match.
- [ ] 2.4 Assemble the release layout with the production manifest, runtime input, final INF/SYS payload, unsigned catalog input, public evidence location, license/notice metadata, and stable relative paths without copying private signing material or `artifacts/` content.
- [x] 2.5 Generate the catalog from the final packaged INF and SYS bytes for the Windows 11 x64 target and record the catalog member set and hashes before any external signing step.

## 3. Formal signing and package verification

- [x] 3.1 Implement the external production signing boundary for the final catalog and any required embedded SYS signature without accepting or persisting private key containers in the repository or release output.
- [ ] 3.2 Generate `trust/signing-evidence.json` with the production route identifier, signer subject and thumbprint, public chain metadata, signature/timestamp evidence, tool versions, package hashes, catalog coverage, and verification results.
- [x] 3.3 Extend the read-only `mini-aec-release` package preflight to validate package evidence schema/version, exact manifest paths, source/build provenance, canonical payload digests, INF/SYS catalog membership, CAT production trust, optional embedded SYS trust, compatibility, and development-separation markers.
- [x] 3.4 Make package verification fail closed on missing or mismatched INF/SYS members, unsigned or development CATs, unapproved signer evidence, stale package hashes, unsupported schema, test-signing requirements, validation identities, unsafe paths, or private signing suffixes.
- [x] 3.5 Ensure package build, signing, and preflight commands remain free of driver installation, certificate-store writes, BCD/TESTSIGNING changes, device/default-role mutations, restart, shutdown, and sign-out operations.

## 4. Reproducibility and repository verification

- [x] 4.1 Add unit tests for package evidence parsing, fixed Windows 11 x64 and transport identity, production/development separation, safe relative paths, private-material rejection, and actionable failure reporting.
- [x] 4.2 Add synthetic package verification tests for tampered INF/SYS content, catalog-member mismatch, invalid CAT trust, changed source/toolchain/build input, variable signature metadata, and a valid lifecycle-compatible evidence report.
- [x] 4.3 Run two clean production payload builds or an equivalent independent replay and verify identical canonical INF/SYS digests, catalog coverage, source provenance, and declared build inputs before accepting reproducibility.
- [ ] 4.4 Run the documented read-only package preflight against the signed candidate and record metadata-only results for manifest, provenance, endpoint scope, INF/SYS/CAT trust, compatibility, privacy, and development-separation checks.
- [ ] 4.5 Run `cargo fmt --all -- --check`, `.tools\cargo-webrtc.cmd test --workspace`, and `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`, plus the clean x64 production package verification commands, without installing or restarting Windows.

## 5. Lifecycle handoff and release readiness

- [ ] 5.1 Verify that a passing package-preflight report and manifest are consumable by `production-driver-lifecycle` before its authorization or staging state, while package verification alone never reports endpoint activation or installation success.
- [ ] 5.2 Review the final package tree and evidence for complete INF/SYS/CAT trust coverage, fixed SysVAD provenance, public-only signing metadata, absence of development identities, absence of private material, and absence of private audio or meeting content.
- [x] 5.3 Publish the package reproducibility and signing verification instructions, known external prerequisites, and explicit stop point before any separately authorized Windows installation, activation, rollback, uninstall, or user-performed restart boundary.
