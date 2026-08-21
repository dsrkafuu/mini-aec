[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageRoot,
    [Parameter(Mandatory)][string]$SignerThumbprint,
    [string]$CheckoutRoot = (Join-Path (Get-Location) '.tools\sysvad-upstream'),
    [string]$SignToolPath,
    [string]$TimestampUrl,
    [ValidateSet('CurrentUser', 'LocalMachine')][string]$CertificateStore = 'CurrentUser',
    [string]$ReplayPackageRoot
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$preflightScript = Join-Path $scriptRoot 'preflight.ps1'
$verifyScript = Join-Path $scriptRoot 'verify-production-package.ps1'
. (Join-Path $scriptRoot 'production-package-common.ps1')

$resolvedPackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
Assert-PackagePathSafety -PackageRoot $resolvedPackageRoot
$catalogPath = Join-Path $resolvedPackageRoot 'driver\MiniAECProduction.cat'
if (-not (Test-Path -LiteralPath $catalogPath -PathType Leaf)) {
    throw "Production catalog is missing: $catalogPath"
}
if ([string]::IsNullOrWhiteSpace($SignerThumbprint)) {
    throw 'A production signer thumbprint or external signer-selected certificate identity is required.'
}
if ($SignerThumbprint -match '(?i)test|development|validation') {
    throw 'A development-looking signer identity cannot be used for production package signing.'
}

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

$signArguments = @('sign', '/v', '/fd', 'SHA256', '/sha1', $SignerThumbprint.Replace(' ', ''), '/s', 'My')
if ($CertificateStore -eq 'LocalMachine') {
    $signArguments += '/sm'
}
if ($TimestampUrl) {
    $signArguments += @('/tr', $TimestampUrl, '/td', 'SHA256')
}
$signArguments += $catalogPath

& $SignToolPath @signArguments
if ($LASTEXITCODE -ne 0) {
    throw "Production catalog signing failed with exit code $LASTEXITCODE."
}

$verifyArguments = @(
    '-PackageRoot', $resolvedPackageRoot,
    '-CheckoutRoot', $CheckoutRoot,
    '-SignToolPath', $SignToolPath,
    '-ExpectedSignerThumbprint', $SignerThumbprint,
    '-RequireProductionSignature'
)
if ($ReplayPackageRoot) {
    $verifyArguments += @('-ReplayPackageRoot', $ReplayPackageRoot)
}
$reportPath = Join-Path ([System.IO.Path]::GetTempPath()) "mini-aec-production-verify-$([guid]::NewGuid().ToString('N')).json"
try {
    $verifyArguments += @('-ReportPath', $reportPath)
    & $verifyScript @verifyArguments
    if ($LASTEXITCODE -ne 0) {
        throw 'Production catalog was signed, but post-signature package verification failed.'
    }
    $report = Read-JsonDocument -Path $reportPath
    $finalManifest = $report.manifest
    Write-JsonDocument -Value $finalManifest -Path (Join-Path $resolvedPackageRoot 'manifest.json')

    $finalEvidence = $report.evidence
    $finalFiles = @(Get-PackageFileEvidenceSet -PackageRoot $resolvedPackageRoot -Manifest $finalManifest)
    $finalMembers = [ordered]@{
        $finalManifest.files.driver_inf = (Get-RecordByPath -Records $finalFiles -Path $finalManifest.files.driver_inf).sha256
        $finalManifest.files.driver_binary = (Get-RecordByPath -Records $finalFiles -Path $finalManifest.files.driver_binary).sha256
    }
    $finalEvidence.files = @($finalFiles)
    $finalEvidence.catalog.path = [string]$finalManifest.files.driver_catalog
    $finalEvidence.catalog.members = [pscustomobject]$finalMembers
    $finalEvidence.catalog.coverage_verified = [bool]$finalEvidence.signing.inf_catalog_coverage_verified -and [bool]$finalEvidence.signing.sys_catalog_coverage_verified
    $finalEvidence.reproducibility.canonical_payload_sha256 = Get-CanonicalPayloadDigest -Records $finalFiles -InfPath $finalManifest.files.driver_inf -SysPath $finalManifest.files.driver_binary
    $finalEvidence.reproducibility.catalog_member_set_sha256 = Get-CanonicalCatalogMemberDigest -Members $finalMembers
    Write-JsonDocument -Value $finalEvidence -Path (Join-Path $resolvedPackageRoot 'trust\signing-evidence.json')

    $finalVerifyArguments = @(
        '-PackageRoot', $resolvedPackageRoot,
        '-CheckoutRoot', $CheckoutRoot,
        '-SignToolPath', $SignToolPath,
        '-ExpectedSignerThumbprint', $SignerThumbprint,
        '-RequireProductionSignature'
    )
    if ($ReplayPackageRoot) {
        $finalVerifyArguments += @('-ReplayPackageRoot', $ReplayPackageRoot)
    }
    & $verifyScript @finalVerifyArguments
    if ($LASTEXITCODE -ne 0) {
        throw 'Final package verification failed after writing public signing evidence.'
    }
} finally {
    if (Test-Path -LiteralPath $reportPath -PathType Leaf) {
        Remove-Item -LiteralPath $reportPath -Force
    }
}

if ($ReplayPackageRoot) {
    Write-Host 'Production catalog signing, INF/SYS catalog coverage, and independent replay verification completed.'
} else {
    Write-Host 'Production catalog signing and INF/SYS catalog coverage completed; run verify-production-package.ps1 with -ReplayPackageRoot before release.'
}
Write-Host 'The signing boundary did not install a certificate or driver and did not change boot, device, default-audio, restart, shutdown, or sign-out state.'
