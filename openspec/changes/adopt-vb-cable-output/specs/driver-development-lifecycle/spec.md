# Spec Delta

## REMOVED Requirements

### Requirement: Auditable SysVAD baseline
**Reason**: The pinned SysVAD source is no longer an active product dependency after the VB-CABLE pivot.
**Migration**: Preserve the existing upstream record and Git history as historical evidence; do not carry SysVAD into the supported product route.

### Requirement: Reproducible validation driver build
**Reason**: MiniAEC will no longer build a project-owned Windows audio driver for product validation.
**Migration**: Validate the user-mode VB-CABLE output adapter with synthetic tests and an already installed external prerequisite.

### Requirement: Explicit approval for system changes
**Reason**: MiniAEC's workflow will no longer perform test-mode, certificate, driver-package or device lifecycle mutations.
**Migration**: Keep the absolute prohibition on agent-initiated operating-system restart in repository policy, while external VB-CABLE installation and removal remain manual user actions outside MiniAEC.

### Requirement: Non-elevated runtime acceptance
**Reason**: Runtime acceptance no longer depends on access to a MiniAEC kernel control interface.
**Migration**: Verify non-elevated access to the selected VB-CABLE playback endpoint and ordinary-client consumption from its paired recording endpoint.

### Requirement: Test-signed installation isolation and default eligibility
**Reason**: MiniAEC will not ship or install a test-signed or production-signed audio endpoint.
**Migration**: The user installs VB-CABLE from the official source and explicitly selects `CABLE Output` in downstream applications.

### Requirement: Clean driver restart
**Reason**: Restarting a project-owned development driver is no longer part of MiniAEC validation.
**Migration**: Exercise output-endpoint invalidation and explicit MiniAEC restart without restarting or mutating the external driver.

### Requirement: Complete uninstall recovery
**Reason**: MiniAEC will not uninstall or roll back VB-CABLE or any other external driver.
**Migration**: Remove the legacy MiniAEC driver artifacts from the repository after VB-CABLE acceptance, while leaving external driver removal entirely to the user and vendor instructions.
