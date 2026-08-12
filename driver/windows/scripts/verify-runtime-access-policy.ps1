[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$securityPath = Join-Path $driverRoot 'mini-aec\MiniAecSecurity.h'
$transportPath = Join-Path $driverRoot 'mini-aec\MiniAecTransport.cpp'
$protocolPath = Join-Path $driverRoot 'mini-aec\MiniAecProtocol.h'
$infPath = Join-Path $driverRoot 'mini-aec\MiniAECValidation.inx'
$runtimeValidationPath = Join-Path $scriptRoot 'runtime-access-validation.ps1'

foreach ($path in @($securityPath, $transportPath, $protocolPath, $infPath, $runtimeValidationPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required MiniAEC driver source is missing: $path"
    }
}

$securityText = Get-Content -LiteralPath $securityPath -Raw
$transportText = Get-Content -LiteralPath $transportPath -Raw
$protocolText = Get-Content -LiteralPath $protocolPath -Raw
$infText = Get-Content -LiteralPath $infPath -Raw
$runtimeValidationText = Get-Content -LiteralPath $runtimeValidationPath -Raw

$expectedSddl = '#define MINIAEC_TRANSPORT_SDDL L"D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)"'
if (-not $securityText.Contains($expectedSddl)) {
    throw 'MiniAEC transport SDDL does not match the protected SYSTEM/Administrators/Interactive Users policy.'
}

foreach ($forbiddenGrant in @(';;;WD)', ';;;AU)', ';;;BU)', ';;;AN)', ';;;BG)', ';;;NU)')) {
    if ($securityText.Contains($forbiddenGrant)) {
        throw "MiniAEC transport SDDL contains a forbidden broader principal grant: $forbiddenGrant"
    }
}

if ($transportText -notmatch 'RtlInitUnicodeString\(&securityDescriptor,\s*MINIAEC_TRANSPORT_SDDL\)') {
    throw 'MiniAEC control device does not consume the project-owned transport SDDL definition.'
}
if ($transportText -notmatch '(?s)IoCreateDeviceSecure\(.*?FILE_DEVICE_SECURE_OPEN,\s*FALSE,\s*&securityDescriptor') {
    throw 'MiniAEC control device must be nonexclusive at the I/O manager layer.'
}
if ($transportText -notmatch '(?s)g_State\.OwnerFile\s*!=\s*nullptr.*?status\s*=\s*STATUS_DEVICE_BUSY') {
    throw 'MiniAEC create dispatch does not retain explicit single-owner busy arbitration.'
}
if ($transportText -notmatch '(?s)NTSTATUS ReleaseOwner.*?CloseSessionLocked\(\);.*?g_State\.OwnerFile\s*=\s*nullptr') {
    throw 'MiniAEC owner cleanup does not close the session and release the owner.'
}
if ($transportText -notmatch '(?s)MiniAecTransportShutdown.*?CloseSessionLocked\(\);.*?g_State\.OwnerFile\s*=\s*nullptr') {
    throw 'MiniAEC shutdown does not clear session audio and release the owner.'
}

$protocolContracts = @(
    '#define MINIAEC_PROTOCOL_VERSION 1U',
    '#define MINIAEC_DIAGNOSTICS_SCHEMA_VERSION 2U',
    '#define MINIAEC_SAMPLE_RATE 48000UL',
    '#define MINIAEC_CHANNELS 1U',
    '#define MINIAEC_BITS_PER_SAMPLE 16U',
    '#define MINIAEC_FRAME_SAMPLES 480U',
    '#define MINIAEC_FRAME_BYTES 960U',
    '#define MINIAEC_RING_CAPACITY 10U',
    'C_ASSERT(sizeof(MINIAEC_OPEN_SESSION_REQUEST) == 40);',
    'C_ASSERT(sizeof(MINIAEC_WRITE_FRAME_REQUEST) == 1004);',
    'C_ASSERT(sizeof(MINIAEC_CLOSE_SESSION_REQUEST) == 28);',
    'C_ASSERT(sizeof(MINIAEC_DIAGNOSTICS) == 128);'
)
foreach ($contract in $protocolContracts) {
    if (-not $protocolText.Contains($contract)) {
        throw "MiniAEC fixed protocol contract changed or is missing: $contract"
    }
}
if ($infText -notmatch 'MiniAEC Microphone') {
    throw 'MiniAEC validation INF no longer declares the fixed public endpoint name.'
}
if ($infText -match 'KSCATEGORY_RENDER') {
    throw 'MiniAEC validation INF unexpectedly declares a producer-facing render endpoint.'
}

foreach ($forbiddenRuntimeCommand in @('pnputil', 'bcdedit', 'devcon', 'certutil', 'Restart-Computer', 'Stop-Computer', 'shutdown.exe', 'logoff.exe', '-Verb RunAs')) {
    if ($runtimeValidationText -match [regex]::Escape($forbiddenRuntimeCommand)) {
        throw "Normal-user runtime validation contains a forbidden lifecycle or elevation command: $forbiddenRuntimeCommand"
    }
}
foreach ($requiredRuntimeContract in @("[ValidateSet('Plan', 'Identity', 'Transport', 'Contention', 'Bypass', 'Aec')]", 'TokenInspector', 'DeviceSecurityInspector', "'S-1-5-4'", '--access-probe', '--microphone-id', '--render-id')) {
    if (-not $runtimeValidationText.Contains($requiredRuntimeContract)) {
        throw "Normal-user runtime validation is missing a required contract: $requiredRuntimeContract"
    }
}

Write-Host 'MiniAEC runtime access policy verified: protected SY/BA full control, IU read/write, explicit single-owner busy arbitration, fixed protocol and capture-only endpoint.'
