[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$securityPath = Join-Path $driverRoot 'mini-aec\MiniAecSecurity.h'
$transportHeaderPath = Join-Path $driverRoot 'mini-aec\MiniAecTransport.h'
$transportPath = Join-Path $driverRoot 'mini-aec\MiniAecTransport.cpp'
$protocolPath = Join-Path $driverRoot 'mini-aec\MiniAecProtocol.h'
$infPath = Join-Path $driverRoot 'mini-aec\MiniAECValidation.inx'
$peakMeterTablePath = Join-Path $driverRoot 'vendor\sysvad\TabletAudioSample\micintoptable.h'
$peakMeterHandlerPath = Join-Path $driverRoot 'vendor\sysvad\TabletAudioSample\micintopo.cpp'
$runtimeValidationPath = Join-Path $scriptRoot 'runtime-access-validation.ps1'
$longRunValidationPath = Join-Path $scriptRoot 'long-run-validation.ps1'

foreach ($path in @($securityPath, $transportHeaderPath, $transportPath, $protocolPath, $infPath, $peakMeterTablePath, $peakMeterHandlerPath, $runtimeValidationPath, $longRunValidationPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required MiniAEC driver source is missing: $path"
    }
}

$securityText = Get-Content -LiteralPath $securityPath -Raw
$transportHeaderText = Get-Content -LiteralPath $transportHeaderPath -Raw
$transportText = Get-Content -LiteralPath $transportPath -Raw
$protocolText = Get-Content -LiteralPath $protocolPath -Raw
$infText = Get-Content -LiteralPath $infPath -Raw
$peakMeterTableText = Get-Content -LiteralPath $peakMeterTablePath -Raw
$peakMeterHandlerText = Get-Content -LiteralPath $peakMeterHandlerPath -Raw
$runtimeValidationText = Get-Content -LiteralPath $runtimeValidationPath -Raw
$longRunValidationText = Get-Content -LiteralPath $longRunValidationPath -Raw

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
if ($transportHeaderText -notmatch 'MiniAecTransportGetCapturePeakMagnitude') {
    throw 'MiniAEC transport does not expose its capture peak magnitude to the endpoint meter.'
}
if ($transportText -notmatch '\bULONG\s+CapturePeakMagnitude\s*;' -or
    $transportText -notmatch '(?s)VOID ResetAudioLocked\(\)\s*\{.*?g_State\.CapturePeakMagnitude\s*=\s*0;') {
    throw 'MiniAEC capture peak state is missing or is not cleared when transport audio resets.'
}
if ($transportText -notmatch '(?s)MiniAecTransportReadCapture\s*\(.*?for\s*\(ULONG sampleOffset\s*=.*?RtlCopyMemory\(&sample,\s*Buffer\s*\+\s*sampleOffset,\s*sizeof\(sample\)\).*?g_State\.CapturePeakMagnitude\s*=\s*peakMagnitude;') {
    throw 'MiniAEC capture peak is not measured from the PCM bytes returned to WaveRT.'
}
if ($peakMeterTableText -notmatch '(?s)KSPROPERTY_AUDIO_PEAKMETER2\s*,\s*KSPROPERTY_TYPE_GET\s*\|\s*KSPROPERTY_TYPE_BASICSUPPORT\s*,\s*PropertyHandler_MiniAecPeakMeter') {
    throw 'MiniAEC topology peak-meter property is not routed through the live PCM meter handler.'
}
if ($peakMeterHandlerText -notmatch '(?s)PropertyHandler_MiniAecPeakMeter\s*\(.*?PropertyHandler_BasicSupportPeakMeter2\s*\(\s*PropertyRequest\s*,\s*MINIAEC_CHANNELS\s*\).*?ValidatePropertyParams\s*\(\s*PropertyRequest\s*,\s*sizeof\(LONG\)\s*,\s*sizeof\(ULONG\)\s*\).*?MiniAecTransportGetCapturePeakMagnitude\s*\(\s*\).*?PEAKMETER_NORMALIZE_IN_RANGE') {
    throw 'MiniAEC topology meter does not validate requests and report the normalized transport peak.'
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
    if ($longRunValidationText -match [regex]::Escape($forbiddenRuntimeCommand)) {
        throw "Long-run runtime validation contains a forbidden lifecycle or elevation command: $forbiddenRuntimeCommand"
    }
}
foreach ($requiredRuntimeContract in @("[ValidateSet('Plan', 'Identity', 'Transport', 'Contention', 'Bypass', 'Aec')]", 'TokenInspector', 'DeviceSecurityInspector', "'S-1-5-4'", '--access-probe', '--microphone-id', '--render-id')) {
    if (-not $runtimeValidationText.Contains($requiredRuntimeContract)) {
        throw "Normal-user runtime validation is missing a required contract: $requiredRuntimeContract"
    }
}

foreach ($requiredLongRunContract in @('runtime-access-validation.ps1', "'-loop', '0'", 'loudnorm=I=-14:TP=-1:LRA=5', "'-f', 'dshow'", 'client-recording.flac', '$consumer.WaitForExit(15000)', 'operator_observation_still_required = $true')) {
    if (-not $longRunValidationText.Contains($requiredLongRunContract)) {
        throw "Long-run runtime validation is missing a required contract: $requiredLongRunContract"
    }
}

Write-Host 'MiniAEC runtime access policy verified: protected SY/BA full control, IU read/write, explicit single-owner busy arbitration, fixed protocol, capture-only endpoint, and live PCM peak-meter wiring.'
