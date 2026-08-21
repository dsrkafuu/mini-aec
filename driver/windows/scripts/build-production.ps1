[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$RuntimeExecutable,
    [string]$CheckoutRoot = (Join-Path (Get-Location) '.tools\sysvad-upstream'),
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$ReleaseVersion = '1.0.0',
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$RuntimeVersion = '1.0.0',
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$DriverVersion = '1.0.0',
    [ValidatePattern('^\d{2}/\d{2}/\d{4}$')][string]$DriverDate = '08/21/2026',
    [string]$OutputRoot
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$sysvadRoot = Join-Path $driverRoot 'vendor\sysvad'
$productionInf = Join-Path $driverRoot 'mini-aec\MiniAECProduction.inx'
$preflightScript = Join-Path $scriptRoot 'preflight.ps1'
$upstreamScript = Join-Path $scriptRoot 'verify-upstream.ps1'
$accessPolicyScript = Join-Path $scriptRoot 'verify-runtime-access-policy.ps1'

if (-not $OutputRoot) {
    $OutputRoot = Join-Path $driverRoot 'out\production-x64-release'
}

$resolvedDriverRoot = [System.IO.Path]::GetFullPath($driverRoot).TrimEnd('\') + '\'
$resolvedOutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
if (-not $resolvedOutputRoot.StartsWith($resolvedDriverRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to write a production package outside the driver workspace: $resolvedOutputRoot"
}

if (-not (Test-Path -LiteralPath $RuntimeExecutable -PathType Leaf)) {
    throw "Runtime executable is missing: $RuntimeExecutable"
}
if ([System.IO.Path]::GetExtension($RuntimeExecutable).ToLowerInvariant() -ne '.exe') {
    throw 'The production release runtime input must be an .exe file.'
}
if (-not (Test-Path -LiteralPath $productionInf -PathType Leaf)) {
    throw "Production INF source is missing: $productionInf"
}

. (Join-Path $scriptRoot 'production-package-common.ps1')

& $upstreamScript -CheckoutRoot $CheckoutRoot
if ($LASTEXITCODE -ne 0) {
    throw 'Pinned SysVAD provenance verification failed before the production build.'
}

& $accessPolicyScript
if ($LASTEXITCODE -ne 0) {
    throw 'The shared MiniAEC transport policy verification failed before the production build.'
}

$preflightJson = & $preflightScript -Json
if ($LASTEXITCODE -ne 0) {
    throw 'Driver toolchain preflight failed. No production package was prepared.'
}
$preflight = $preflightJson | ConvertFrom-Json
$kitVersion = @($preflight.WindowsDriverKitVersions) |
    Where-Object { $_ -in @($preflight.WindowsSdkVersions) } |
    Sort-Object { [version]$_ } -Descending |
    Select-Object -First 1
if (-not $kitVersion) {
    throw 'Driver toolchain preflight did not report a matching Windows SDK/WDK build number.'
}

$commonProject = Join-Path $sysvadRoot 'EndpointsCommon\EndpointsCommon.vcxproj'
$driverProject = Join-Path $sysvadRoot 'TabletAudioSample\TabletAudioSample.vcxproj'
foreach ($project in @($commonProject, $driverProject)) {
    if (-not (Test-Path -LiteralPath $project -PathType Leaf)) {
        throw "Pinned SysVAD project is missing: $project"
    }
}

$generatedPaths = @(
    (Join-Path $sysvadRoot 'EndpointsCommon\x64\Release'),
    (Join-Path $sysvadRoot 'TabletAudioSample\x64\Release'),
    $resolvedOutputRoot
)
foreach ($generatedPath in $generatedPaths) {
    $resolvedGeneratedPath = [System.IO.Path]::GetFullPath($generatedPath)
    if (-not $resolvedGeneratedPath.StartsWith($resolvedDriverRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean a generated path outside the driver workspace: $resolvedGeneratedPath"
    }
    if (Test-Path -LiteralPath $resolvedGeneratedPath) {
        Remove-Item -LiteralPath $resolvedGeneratedPath -Recurse -Force
    }
}

$buildArguments = @(
    '/m',
    '/nologo',
    '/restore:false',
    '/t:Rebuild',
    '/p:Configuration=Release',
    '/p:Platform=x64',
    "/p:WindowsTargetPlatformVersion=$kitVersion",
    '/p:MiniAecReproducible=true',
    "/p:MiniAecDriverDate=$DriverDate",
    "/p:MiniAecDriverVersion=$($DriverVersion).0",
    '/p:SignMode=Off',
    '/verbosity:minimal'
)

& $preflight.MSBuildPath $commonProject @buildArguments
if ($LASTEXITCODE -ne 0) {
    throw "EndpointsCommon production rebuild failed with exit code $LASTEXITCODE."
}

$driverBuildArguments = @(
    $buildArguments +
    "/p:MiniAecInf=$productionInf" +
    '/p:TargetName=MiniAECProduction'
)
& $preflight.MSBuildPath $driverProject @driverBuildArguments
if ($LASTEXITCODE -ne 0) {
    throw "MiniAEC production driver rebuild failed with exit code $LASTEXITCODE."
}

$buildRoot = Join-Path $sysvadRoot 'TabletAudioSample\x64\Release'
$builtInf = Join-Path $buildRoot 'MiniAECProduction.inf'
$builtSys = Join-Path $buildRoot 'MiniAECProduction.sys'
foreach ($artifact in @($builtInf, $builtSys)) {
    if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
        throw "Expected production artifact is missing: $artifact"
    }
}

$infText = Get-Content -LiteralPath $builtInf -Raw
$captureInterfaces = [regex]::Matches($infText, '(?im)^\s*AddInterface\s*=\s*%KSCATEGORY_CAPTURE%')
if ($infText -notmatch 'MiniAEC Microphone' -or $captureInterfaces.Count -ne 1) {
    throw 'Production INF does not declare exactly one required MiniAEC Microphone capture endpoint.'
}
if ($infText -match 'KSCATEGORY_RENDER') {
    throw 'Production INF unexpectedly declares a producer-facing render endpoint.'
}
foreach ($forbiddenIdentity in @('MiniAECValidation', 'validation-x64-debug', 'Development')) {
    if ($infText -match [regex]::Escape($forbiddenIdentity)) {
        throw "Production INF contains development identity text: $forbiddenIdentity"
    }
}

$releaseDriverRoot = Join-Path $resolvedOutputRoot 'driver'
$releaseRuntimeRoot = Join-Path $resolvedOutputRoot 'runtime'
$releaseTrustRoot = Join-Path $resolvedOutputRoot 'trust'
New-Item -ItemType Directory -Path $releaseDriverRoot, $releaseRuntimeRoot, $releaseTrustRoot -Force | Out-Null
Copy-Item -LiteralPath $builtInf -Destination (Join-Path $releaseDriverRoot 'MiniAECProduction.inf')
Copy-Item -LiteralPath $builtSys -Destination (Join-Path $releaseDriverRoot 'MiniAECProduction.sys')

$inf2CatCandidates = @(
    (Join-Path $preflight.KitsRoot10 "bin\$kitVersion\x86\Inf2Cat.exe"),
    (Join-Path $preflight.KitsRoot10 "bin\$kitVersion\x64\Inf2Cat.exe")
)
$inf2Cat = $inf2CatCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $inf2Cat) {
    throw 'Inf2Cat.exe was not found in the selected WDK.'
}
& $inf2Cat "/driver:$releaseDriverRoot" '/os:10_X64' '/uselocaltime'
if ($LASTEXITCODE -ne 0) {
    throw "Inf2Cat failed with exit code $LASTEXITCODE."
}
$catalog = Join-Path $releaseDriverRoot 'MiniAECProduction.cat'
if (-not (Test-Path -LiteralPath $catalog -PathType Leaf)) {
    throw 'Inf2Cat completed without producing MiniAECProduction.cat.'
}

Copy-Item -LiteralPath $RuntimeExecutable -Destination (Join-Path $releaseRuntimeRoot 'mini-aec.exe')
$licensePath = Join-Path $sysvadRoot 'LICENSE'
if (-not (Test-Path -LiteralPath $licensePath -PathType Leaf)) {
    throw "Pinned SysVAD MS-PL notice is missing: $licensePath"
}
Copy-Item -LiteralPath $licensePath -Destination (Join-Path $releaseTrustRoot 'SysVAD-MS-PL.txt')

$manifest = [ordered]@{
    schema_version = 1
    product = [ordered]@{
        name = 'MiniAEC'
        identifier = 'mini-aec'
    }
    release_version = $ReleaseVersion
    target = [ordered]@{
        os = 'windows-11'
        architecture = 'x86_64'
    }
    runtime = [ordered]@{
        identifier = 'mini-aec'
        version = $RuntimeVersion
    }
    driver = [ordered]@{
        identifier = 'mini-aec-windows-driver'
        version = $DriverVersion
    }
    transport = [ordered]@{
        public_endpoint_name = 'MiniAEC Microphone'
        producer_interface_name = 'MiniAECTransport'
        protocol_version = 1
        diagnostics_schema_version = 2
        sample_rate_hz = 48000
        channels = 1
        bits_per_sample = 16
        frame_samples_per_channel = 480
    }
    compatibility = [ordered]@{
        runtime_minimum = $RuntimeVersion
        runtime_maximum = $RuntimeVersion
        driver_minimum = $DriverVersion
        driver_maximum = $DriverVersion
    }
    trust = [ordered]@{
        channel = 'production'
        signer_subject = 'EXTERNAL SIGNING REQUIRED'
        signer_thumbprint = 'EXTERNAL SIGNING REQUIRED'
        signature_verified = $false
        test_signing_required = $false
        private_signing_material_present = $false
    }
    files = [ordered]@{
        runtime_executable = 'runtime/mini-aec.exe'
        driver_inf = 'driver/MiniAECProduction.inf'
        driver_binary = 'driver/MiniAECProduction.sys'
        driver_catalog = 'driver/MiniAECProduction.cat'
        signing_evidence = 'trust/signing-evidence.json'
    }
}

$manifestPath = Join-Path $resolvedOutputRoot 'manifest.json'
Write-JsonDocument -Value $manifest -Path $manifestPath

$commands = @(
    'verify-upstream.ps1 -CheckoutRoot .tools/sysvad-upstream',
    'preflight.ps1 -Json',
    'MSBuild EndpointsCommon.vcxproj /t:Rebuild /p:Configuration=Release /p:Platform=x64 /p:MiniAecReproducible=true /p:MiniAecDriverDate=' + $DriverDate + ' /p:MiniAecDriverVersion=' + $DriverVersion + '.0 /p:SignMode=Off',
    'MSBuild TabletAudioSample.vcxproj /t:Rebuild /p:Configuration=Release /p:Platform=x64 /p:MiniAecInf=MiniAECProduction.inx /p:TargetName=MiniAECProduction /p:MiniAecReproducible=true /p:MiniAecDriverDate=' + $DriverDate + ' /p:MiniAecDriverVersion=' + $DriverVersion + '.0 /p:SignMode=Off',
    'Inf2Cat /os:10_X64 /uselocaltime'
)
$build = New-BuildEvidence -Preflight $preflight -Commands $commands
$signing = [ordered]@{
    route = 'external-production-signing-required'
    signer_subject = 'EXTERNAL SIGNING REQUIRED'
    signer_thumbprint = 'EXTERNAL SIGNING REQUIRED'
    public_chain = @()
    cat_signature_verified = $false
    inf_catalog_coverage_verified = $false
    sys_catalog_coverage_verified = $false
    sys_embedded_signature_required = $false
    sys_embedded_signature_verified = $false
    test_signing_required = $false
    signature_timestamp = 'not-signed'
}
$evidence = New-PackageEvidence `
    -PackageRoot $resolvedOutputRoot `
    -DriverRoot $driverRoot `
    -Manifest $manifest `
    -Build $build `
    -Signing $signing `
    -ReplayVerified $false `
    -VerificationVerified $false `
    -VerificationTool 'verify-production-package.ps1' `
    -VerificationCommands @('verify-production-package.ps1 -PackageRoot <production-release> -AllowUnsigned')
Write-JsonDocument -Value $evidence -Path (Join-Path $releaseTrustRoot 'signing-evidence.json')
Assert-PackagePathSafety -PackageRoot $resolvedOutputRoot

Write-Host "Unsigned x64 Release production package prepared at $resolvedOutputRoot"
Write-Host 'The package is not distributable until an approved external signing route produces a verified CAT and replay evidence.'
Write-Host 'No certificate, driver, device, boot, default-audio, restart, shutdown, or sign-out operation was performed.'
