# Spec Delta

## REMOVED Requirements

### Requirement: Single public virtual microphone endpoint
**Reason**: MiniAEC will no longer ship or own a Windows capture endpoint; the user-installed VB-CABLE recording endpoint becomes the downstream capture surface.
**Migration**: Use the `vb-cable-output` capability and select the resolved `CABLE Output` endpoint in target applications.

### Requirement: Restricted private producer interface
**Reason**: The project-owned kernel control interface is retired with the SysVAD transport.
**Migration**: Render processed audio to the resolved VB-CABLE playback endpoint through the product's user-mode output boundary.

### Requirement: Non-elevated product runtime
**Reason**: Runtime output no longer opens `MiniAECTransport` or depends on a MiniAEC driver ACL.
**Migration**: Validate that the ordinary non-elevated MiniAEC process can open the already installed VB-CABLE playback endpoint through the `vb-cable-output` capability.

### Requirement: Validated fixed-frame protocol
**Reason**: The private IOCTL protocol and its fixed PCM16 packet format are removed from the product route.
**Migration**: Keep 10 ms 48 kHz mono frames at the engine boundary and use bounded deterministic adaptation to the active VB-CABLE playback format.

### Requirement: Driver-owned bounded ring buffer
**Reason**: MiniAEC no longer owns a kernel driver or driver-side ring buffer.
**Migration**: Enforce bounded user-mode output storage and observable backpressure under the `vb-cable-output` capability.

### Requirement: Deterministic PCM injection
**Reason**: Deterministic injection through `MiniAECTransport` and `MiniAEC Microphone` is no longer part of the product.
**Migration**: Validate deterministic rendering to `CABLE Input` and downstream capture from its paired `CABLE Output` endpoint.

### Requirement: Continuous capture
**Reason**: MiniAEC no longer controls the capture clock or transport diagnostics of a project-owned public endpoint.
**Migration**: Measure user-mode rendering continuity and verify the forwarded stream with an ordinary client on `CABLE Output`.

### Requirement: Deterministic sender absence behavior
**Reason**: Silence generation during producer absence belongs to the external VB-CABLE driver and cannot be specified as project-owned kernel behavior.
**Migration**: Require MiniAEC to close its render session and submit no stale PCM; validate observed downstream absence behavior against the supported VB-CABLE version.

### Requirement: Sender restart recovery
**Reason**: Project-owned sender sessions and driver session identities are removed.
**Migration**: Require each MiniAEC restart to create a fresh VB-CABLE render session after clearing all user-mode output state.

### Requirement: Driver restart recovery
**Reason**: MiniAEC no longer owns or restarts a Windows audio driver.
**Migration**: Treat VB-CABLE endpoint invalidation as a terminal run failure and require an explicit user restart after the external endpoint becomes available again.
