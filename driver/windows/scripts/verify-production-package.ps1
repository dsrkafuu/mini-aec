[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageRoot,
    [string]$CheckoutRoot = (Join-Path (Get-Location) '.tools\sysvad-upstream'),
    [string]$SignToolPath,
    [string]$ExpectedSignerThumbprint,
    [string]$ReplayPackageRoot,
    [string]$ReportPath,
    [switch]$AllowUnsigned,
    [switch]$RequireProductionSignature
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$upstreamScript = Join-Path $scriptRoot 'verify-upstream.ps1'
$preflightScript = Join-Path $scriptRoot 'preflight.ps1'
. (Join-Path $scriptRoot 'production-package-common.ps1')

if ($AllowUnsigned -and $RequireProductionSignature) {
    throw 'Choose either -AllowUnsigned or -RequireProductionSignature, not both.'
}
if (-not $AllowUnsigned -and -not $RequireProductionSignature) {
    $RequireProductionSignature = $true
}
if ($RequireProductionSignature -and [string]::IsNullOrWhiteSpace($ExpectedSignerThumbprint)) {
    throw 'Formal production verification requires the approved signer thumbprint.'
}

$resolvedPackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
Assert-PackagePathSafety -PackageRoot $resolvedPackageRoot

if ($ReportPath) {
    $resolvedReportPath = [System.IO.Path]::GetFullPath($ReportPath)
    $packagePrefix = $resolvedPackageRoot.TrimEnd('\') + '\'
    if ($resolvedReportPath.StartsWith($packagePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'A verification report must not be written inside the package root.'
    }
}

$manifestPath = Join-Path $resolvedPackageRoot 'manifest.json'
$evidencePath = Join-Path $resolvedPackageRoot 'trust\signing-evidence.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Production manifest is missing: $manifestPath"
}
if (-not (Test-Path -LiteralPath $evidencePath -PathType Leaf)) {
    throw "Production signing evidence is missing: $evidencePath"
}
$manifest = Read-JsonDocument -Path $manifestPath
$evidence = Read-JsonDocument -Path $evidencePath

function Assert-ExactValue {
    param([string]$Field, [string]$Actual, [string]$Expected)
    if ($Actual -ne $Expected) {
        throw "$Field must be '$Expected', got '$Actual'."
    }
}

Assert-ExactValue -Field 'manifest.product.name' -Actual $manifest.product.name -Expected 'MiniAEC'
Assert-ExactValue -Field 'manifest.product.identifier' -Actual $manifest.product.identifier -Expected 'mini-aec'
Assert-ExactValue -Field 'manifest.schema_version' -Actual ([string]$manifest.schema_version) -Expected '1'
Assert-ExactValue -Field 'evidence.schema_version' -Actual ([string]$evidence.schema_version) -Expected '1'
Assert-ExactValue -Field 'evidence.package_identifier' -Actual $evidence.package_identifier -Expected 'mini-aec'
Assert-ExactValue -Field 'evidence.package_version' -Actual $evidence.package_version -Expected $manifest.release_version
Assert-ExactValue -Field 'manifest.target.os' -Actual $manifest.target.os -Expected 'windows-11'
Assert-ExactValue -Field 'manifest.target.architecture' -Actual $manifest.target.architecture -Expected 'x86_64'
Assert-ExactValue -Field 'manifest.driver.identifier' -Actual $manifest.driver.identifier -Expected 'mini-aec-windows-driver'
Assert-ExactValue -Field 'manifest.transport.public_endpoint_name' -Actual $manifest.transport.public_endpoint_name -Expected 'MiniAEC Microphone'
Assert-ExactValue -Field 'manifest.transport.producer_interface_name' -Actual $manifest.transport.producer_interface_name -Expected 'MiniAECTransport'
if ($manifest.transport.protocol_version -ne 1 -or $manifest.transport.diagnostics_schema_version -ne 2 -or $manifest.transport.sample_rate_hz -ne 48000 -or $manifest.transport.channels -ne 1 -or $manifest.transport.bits_per_sample -ne 16 -or $manifest.transport.frame_samples_per_channel -ne 480) {
    throw 'Production manifest transport contract does not match MiniAEC protocol version 1.'
}
Assert-ExactValue -Field 'manifest.files.driver_inf' -Actual $manifest.files.driver_inf -Expected 'driver/MiniAECProduction.inf'
Assert-ExactValue -Field 'manifest.files.driver_binary' -Actual $manifest.files.driver_binary -Expected 'driver/MiniAECProduction.sys'
Assert-ExactValue -Field 'manifest.files.driver_catalog' -Actual $manifest.files.driver_catalog -Expected 'driver/MiniAECProduction.cat'
Assert-ExactValue -Field 'manifest.files.signing_evidence' -Actual $manifest.files.signing_evidence -Expected 'trust/signing-evidence.json'
Assert-ExactValue -Field 'manifest.trust.channel' -Actual $manifest.trust.channel -Expected 'production'
if ([bool]$manifest.trust.test_signing_required -or [bool]$manifest.trust.private_signing_material_present) {
    throw 'Production manifest contains a TESTSIGNING or private signing requirement.'
}
if (-not [bool]$evidence.reproducibility.signature_bytes_excluded) {
    throw 'Package reproducibility evidence must exclude variable detached signature bytes.'
}

$infPath = Join-Path $resolvedPackageRoot 'driver\MiniAECProduction.inf'
$sysPath = Join-Path $resolvedPackageRoot 'driver\MiniAECProduction.sys'
$catalogPath = Join-Path $resolvedPackageRoot 'driver\MiniAECProduction.cat'
$noticePath = Join-Path $resolvedPackageRoot 'trust\SysVAD-MS-PL.txt'
foreach ($path in @($infPath, $sysPath, $catalogPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Production driver payload is missing: $path"
    }
}
$upstreamNoticePath = Join-Path $driverRoot 'vendor\sysvad\LICENSE'
if (-not (Test-Path -LiteralPath $noticePath -PathType Leaf)) {
    throw "Public SysVAD license notice is missing: $noticePath"
}
if (-not (Test-Path -LiteralPath $upstreamNoticePath -PathType Leaf) -or (Get-Sha256 -Path $noticePath) -ne (Get-Sha256 -Path $upstreamNoticePath)) {
    throw 'The packaged SysVAD license notice does not match the pinned source notice.'
}

$infText = Get-Content -LiteralPath $infPath -Raw
$captureInterfaces = [regex]::Matches($infText, '(?im)^\s*AddInterface\s*=\s*%KSCATEGORY_CAPTURE%')
if ($infText -notmatch 'MiniAEC Microphone' -or $captureInterfaces.Count -ne 1) {
    throw 'Production INF does not declare exactly one MiniAEC Microphone capture interface.'
}
if ($infText -match 'KSCATEGORY_RENDER') {
    throw 'Production INF unexpectedly declares a render interface.'
}
foreach ($forbiddenIdentity in @('MiniAECValidation', 'validation-x64-debug', 'Development')) {
    if ($infText -match [regex]::Escape($forbiddenIdentity)) {
        throw "Production INF contains development identity text: $forbiddenIdentity"
    }
}

& $upstreamScript -CheckoutRoot $CheckoutRoot
if ($LASTEXITCODE -ne 0) {
    throw 'Pinned SysVAD provenance verification failed.'
}
$source = New-SourceEvidence -DriverRoot $driverRoot
foreach ($field in @('repository', 'path', 'commit', 'license', 'record_path', 'source_tree_sha256', 'imported_file_set_sha256')) {
    if ([string]$evidence.source.$field -ne [string]$source.$field) {
        throw "Package source evidence differs at source.$field."
    }
}
$expectedPatchPaths = @($script:ProductionLocalPatchPaths | Sort-Object)
$actualPatchPaths = @($evidence.source.local_patches | ForEach-Object { $_.path } | Sort-Object)
$patchDifference = @(Compare-Object -ReferenceObject $expectedPatchPaths -DifferenceObject $actualPatchPaths)
if ($patchDifference.Count -ne 0) {
    throw 'Package source evidence local-patch set does not match the pinned patch ledger.'
}
foreach ($patch in @($source.local_patches)) {
    $recordedPatch = @($evidence.source.local_patches | Where-Object { $_.path -eq $patch.path }) | Select-Object -First 1
    if (-not $recordedPatch -or $recordedPatch.sha256 -ne $patch.sha256) {
        throw "Package source evidence hash differs for local patch $($patch.path)."
    }
}

$files = @(Get-PackageFileEvidenceSet -PackageRoot $resolvedPackageRoot -Manifest $manifest)
$expectedFilePaths = @($files | ForEach-Object { $_.path } | Sort-Object)
$actualFilePaths = @($evidence.files | ForEach-Object { $_.path } | Sort-Object)
$fileDifference = @(Compare-Object -ReferenceObject $expectedFilePaths -DifferenceObject $actualFilePaths)
if ($fileDifference.Count -ne 0) {
    throw 'Package file evidence set does not match the production release layout.'
}
foreach ($actualFile in $files) {
    $record = @($evidence.files | Where-Object { $_.path -eq $actualFile.path }) | Select-Object -First 1
    if (-not $record) {
        throw "Package evidence has no file record for $($actualFile.path)."
    }
    if ($record.kind -ne $actualFile.kind -or $record.sha256 -ne $actualFile.sha256 -or [int64]$record.size_bytes -ne [int64]$actualFile.size_bytes) {
        throw "Package evidence hash or size differs for $($actualFile.path)."
    }
}
$expectedPayloadDigest = Get-CanonicalPayloadDigest -Records $files -InfPath $manifest.files.driver_inf -SysPath $manifest.files.driver_binary
$expectedCatalogMembers = [ordered]@{
    $manifest.files.driver_inf = (Get-RecordByPath -Records $files -Path $manifest.files.driver_inf).sha256
    $manifest.files.driver_binary = (Get-RecordByPath -Records $files -Path $manifest.files.driver_binary).sha256
}
$expectedCatalogDigest = Get-CanonicalCatalogMemberDigest -Members $expectedCatalogMembers
if ($evidence.reproducibility.canonical_payload_sha256 -ne $expectedPayloadDigest) {
    throw 'Package reproducibility payload digest does not match the final INF/SYS bytes.'
}
if ($evidence.reproducibility.catalog_member_set_sha256 -ne $expectedCatalogDigest) {
    throw 'Package catalog member digest does not match the final INF/SYS bytes.'
}

$actualCatalogMembers = [ordered]@{
    $manifest.files.driver_inf = (Get-RecordByPath -Records $files -Path $manifest.files.driver_inf).sha256
    $manifest.files.driver_binary = (Get-RecordByPath -Records $files -Path $manifest.files.driver_binary).sha256
}
if ($evidence.catalog.members.PSObject.Properties.Name -notcontains $manifest.files.driver_inf -or $evidence.catalog.members.PSObject.Properties.Name -notcontains $manifest.files.driver_binary) {
    throw 'Package evidence catalog coverage does not contain both final INF and SYS members.'
}
foreach ($member in $actualCatalogMembers.Keys) {
    $memberProperty = $evidence.catalog.members.PSObject.Properties[$member]
    if (-not $memberProperty -or $memberProperty.Value -ne $actualCatalogMembers[$member]) {
        throw "Package catalog evidence hash differs for $member."
    }
}

if ([string]::IsNullOrWhiteSpace([string]$evidence.build.windows) -or [string]$evidence.build.configuration -ne 'Release' -or [string]$evidence.build.platform -ne 'x64' -or @($evidence.build.commands).Count -eq 0) {
    throw 'Package build provenance is incomplete or not an x64 Release build.'
}
foreach ($command in @($evidence.build.commands) + @($evidence.verification.commands)) {
    $normalized = ([string]$command).ToLowerInvariant()
    foreach ($forbidden in @('pnputil', 'devcon', 'bcdedit', 'certutil', 'restart-computer', 'stop-computer', 'shutdown.exe', 'logoff.exe', '-verb runas')) {
        if ($normalized.Contains($forbidden)) {
            throw "Package evidence contains a forbidden machine-changing command: $command"
        }
    }
}

$replayVerified = $false
if ($ReplayPackageRoot) {
    $replayRoot = (Resolve-Path -LiteralPath $ReplayPackageRoot).Path
    $replayEvidencePath = Join-Path $replayRoot 'trust\signing-evidence.json'
    if (-not (Test-Path -LiteralPath $replayEvidencePath -PathType Leaf)) {
        throw "Replay package evidence is missing: $replayEvidencePath"
    }
    $replayEvidence = Read-JsonDocument -Path $replayEvidencePath
    if ($replayEvidence.reproducibility.canonical_payload_sha256 -ne $expectedPayloadDigest -or $replayEvidence.reproducibility.catalog_member_set_sha256 -ne $expectedCatalogDigest) {
        throw 'Independent replay does not reproduce the canonical INF/SYS payload or catalog member set.'
    }
    $replayVerified = $true
}

$signer = $null
$signerChain = @()
$signatureTimestamp = 'not-signed'
$catSignatureVerified = $false
$infCatalogCoverageVerified = $false
$sysCatalogCoverageVerified = $false
$sysEmbeddedSignatureVerified = $false
$sysEmbeddedSignatureRequired = [bool]$evidence.signing.sys_embedded_signature_required

if ($RequireProductionSignature) {
    if (-not $SignToolPath) {
        $preflightJson = & $preflightScript -Json
        if ($LASTEXITCODE -ne 0) {
            throw 'Driver toolchain preflight failed while locating the production SignTool.'
        }
        $preflight = $preflightJson | ConvertFrom-Json
        $SignToolPath = [string]$preflight.SignToolPath
    }
    if (-not (Test-Path -LiteralPath $SignToolPath -PathType Leaf)) {
        throw "SignTool is missing: $SignToolPath"
    }

    & $SignToolPath verify '/v' '/kp' '/all' $catalogPath
    if ($LASTEXITCODE -ne 0) {
        throw "Production CAT signature verification failed with exit code $LASTEXITCODE."
    }
    foreach ($memberPath in @($infPath, $sysPath)) {
        & $SignToolPath verify '/v' '/kp' '/c' $catalogPath $memberPath
        if ($LASTEXITCODE -ne 0) {
            throw "Production catalog coverage verification failed for $memberPath with exit code $LASTEXITCODE."
        }
    }
    $catSignature = Get-AuthenticodeSignature -FilePath $catalogPath
    if ($catSignature.Status -ne 'Valid' -or -not $catSignature.SignerCertificate) {
        throw "Production CAT Authenticode status is not Valid: $($catSignature.Status)."
    }
    $signer = $catSignature.SignerCertificate
    $actualThumbprint = $signer.Thumbprint.Replace(' ', '').ToUpperInvariant()
    if ($ExpectedSignerThumbprint -and $actualThumbprint -ne $ExpectedSignerThumbprint.Replace(' ', '').ToUpperInvariant()) {
        throw "Production signer thumbprint differs from the expected release signer: $actualThumbprint"
    }
    if ($signer.Subject -match '(?i)test|development|validation') {
        throw "Production CAT is signed by a development-looking identity: $($signer.Subject)"
    }
    $chain = New-Object System.Security.Cryptography.X509Certificates.X509Chain
    $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck
    $chainBuilt = $chain.Build($signer)
    if (-not $chainBuilt) {
        $chainStatus = @($chain.ChainStatus | ForEach-Object { $_.StatusInformation.Trim() }) -join '; '
        throw "Production CAT signer chain could not be built: $chainStatus"
    }
    foreach ($element in $chain.ChainElements) {
        $certificate = $element.Certificate
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            $certificateHash = (($sha.ComputeHash($certificate.RawData) | ForEach-Object { $_.ToString('x2') }) -join '')
        } finally {
            $sha.Dispose()
        }
        $signerChain += [ordered]@{
            subject = $certificate.Subject
            issuer = $certificate.Issuer
            thumbprint = $certificate.Thumbprint.Replace(' ', '').ToUpperInvariant()
            sha256 = $certificateHash
        }
    }
    $timestampCertificate = if ($catSignature.PSObject.Properties.Name -contains 'TimeStamperCertificate') {
        $catSignature.TimeStamperCertificate
    } else {
        $null
    }
    $signatureTimestamp = if ($timestampCertificate) {
        "timestamp signer: $($timestampCertificate.Subject)"
    } else {
        'none'
    }
    $catSignatureVerified = $true
    $infCatalogCoverageVerified = $true
    $sysCatalogCoverageVerified = $true
}

if ($sysEmbeddedSignatureRequired) {
    $sysSignature = Get-AuthenticodeSignature -FilePath $sysPath
    if ($sysSignature.Status -ne 'Valid' -or -not $sysSignature.SignerCertificate) {
        throw "Required embedded SYS signature is not valid: $($sysSignature.Status)."
    }
    if ($ExpectedSignerThumbprint -and $sysSignature.SignerCertificate.Thumbprint.Replace(' ', '').ToUpperInvariant() -ne $ExpectedSignerThumbprint.Replace(' ', '').ToUpperInvariant()) {
        throw 'Required embedded SYS signature signer differs from the expected release signer.'
    }
    $sysEmbeddedSignatureVerified = $true
}

if (-not $RequireProductionSignature -and -not $AllowUnsigned) {
    throw 'Production signature verification mode was not selected.'
}

$signing = [ordered]@{
    route = if ($RequireProductionSignature) { 'external-production-signing' } else { 'external-production-signing-required' }
    signer_subject = if ($signer) { $signer.Subject } else { 'EXTERNAL SIGNING REQUIRED' }
    signer_thumbprint = if ($signer) { $signer.Thumbprint.Replace(' ', '').ToUpperInvariant() } else { 'EXTERNAL SIGNING REQUIRED' }
    public_chain = @($signerChain)
    cat_signature_verified = $catSignatureVerified
    inf_catalog_coverage_verified = $infCatalogCoverageVerified
    sys_catalog_coverage_verified = $sysCatalogCoverageVerified
    sys_embedded_signature_required = $sysEmbeddedSignatureRequired
    sys_embedded_signature_verified = $sysEmbeddedSignatureVerified
    test_signing_required = $false
    signature_timestamp = $signatureTimestamp
}

$verificationCommands = @(
    'verify-upstream.ps1 -CheckoutRoot .tools/sysvad-upstream',
    'verify-production-package.ps1 -PackageRoot <production-release> -RequireProductionSignature'
)
if ($RequireProductionSignature) {
    $verificationCommands += @(
        'signtool verify /v /kp /all driver/MiniAECProduction.cat',
        'signtool verify /v /kp /c driver/MiniAECProduction.cat driver/MiniAECProduction.inf',
        'signtool verify /v /kp /c driver/MiniAECProduction.cat driver/MiniAECProduction.sys'
    )
}
if ($ReplayPackageRoot) {
    $verificationCommands += 'verify-production-package.ps1 -PackageRoot <production-release> -ReplayPackageRoot <independent-replay>'
}
$trustVerified = $catSignatureVerified -and $infCatalogCoverageVerified -and $sysCatalogCoverageVerified -and (-not $sysEmbeddedSignatureRequired -or $sysEmbeddedSignatureVerified)
$verificationVerified = $RequireProductionSignature -and $trustVerified -and $replayVerified
$updatedEvidence = $evidence
$updatedEvidence.files = @($files)
$updatedEvidence.source = $source
$updatedEvidence.catalog.path = [string]$manifest.files.driver_catalog
$updatedEvidence.catalog.members = [pscustomobject]$actualCatalogMembers
$updatedEvidence.catalog.coverage_verified = $infCatalogCoverageVerified -and $sysCatalogCoverageVerified
$updatedEvidence.reproducibility.canonical_payload_sha256 = $expectedPayloadDigest
$updatedEvidence.reproducibility.catalog_member_set_sha256 = $expectedCatalogDigest
$updatedEvidence.reproducibility.replay_verified = $replayVerified
$updatedEvidence.reproducibility.signature_bytes_excluded = $true
$updatedEvidence.signing = [pscustomobject]$signing
$updatedEvidence.verification.verified = $verificationVerified
$updatedEvidence.verification.replay_verified = $replayVerified
$updatedEvidence.verification.read_only = $true
$updatedEvidence.verification.tool = 'verify-production-package.ps1'
$updatedEvidence.verification.commands = @($verificationCommands)
$updatedEvidence.privacy.private_signing_material_present = $false
$updatedEvidence.privacy.audio_content_present = $false
$updatedEvidence.privacy.artifacts_content_present = $false
$updatedEvidence.privacy.machine_secret_material_present = $false

$updatedManifest = $manifest
$updatedManifest.trust.channel = 'production'
$updatedManifest.trust.signer_subject = $signing.signer_subject
$updatedManifest.trust.signer_thumbprint = $signing.signer_thumbprint
$updatedManifest.trust.signature_verified = $verificationVerified
$updatedManifest.trust.test_signing_required = $false
$updatedManifest.trust.private_signing_material_present = $false
if ($ReportPath) {
    $reportDirectory = Split-Path -Parent $resolvedReportPath
    if (-not (Test-Path -LiteralPath $reportDirectory -PathType Container)) {
        throw "Verification report directory is missing: $reportDirectory"
    }
    Write-JsonDocument -Value ([ordered]@{
        package_root = $resolvedPackageRoot
        manifest = $updatedManifest
        evidence = $updatedEvidence
    }) -Path $resolvedReportPath
}

if ($RequireProductionSignature -and $verificationVerified) {
    Write-Host "Production CAT, INF/SYS catalog coverage, and replay verification passed for $resolvedPackageRoot"
} elseif ($AllowUnsigned) {
    Write-Host "Unsigned production package structure and reproducibility inputs verified at $resolvedPackageRoot"
    Write-Host 'The package remains non-distributable until an approved production CAT signature and independent replay are supplied.'
}
Write-Host 'Package verification is read-only; no certificate-store, driver, device, boot, default-audio, restart, shutdown, or sign-out operation was performed.'
