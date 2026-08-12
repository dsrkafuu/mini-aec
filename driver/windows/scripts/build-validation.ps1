[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$sysvadRoot = Join-Path $driverRoot 'vendor\sysvad'
$preflightScript = Join-Path $scriptRoot 'preflight.ps1'
$accessPolicyScript = Join-Path $scriptRoot 'verify-runtime-access-policy.ps1'
$packageRoot = Join-Path $driverRoot 'out\validation-x64-debug'

& $accessPolicyScript

$preflightJson = & $preflightScript -Json
if ($LASTEXITCODE -ne 0) {
    throw 'Driver toolchain preflight failed. Install the reported prerequisites before retrying the validation build.'
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

$resolvedDriverRoot = [System.IO.Path]::GetFullPath($driverRoot).TrimEnd('\') + '\'
foreach ($generatedPath in @(
    (Join-Path $sysvadRoot 'EndpointsCommon\x64\Debug'),
    (Join-Path $sysvadRoot 'TabletAudioSample\x64\Debug'),
    $packageRoot
)) {
    $resolvedGeneratedPath = [System.IO.Path]::GetFullPath($generatedPath)
    if (-not $resolvedGeneratedPath.StartsWith($resolvedDriverRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean a generated path outside the driver workspace: $resolvedGeneratedPath"
    }
    if (Test-Path -LiteralPath $resolvedGeneratedPath) {
        Remove-Item -LiteralPath $resolvedGeneratedPath -Recurse -Force
    }
}

$arguments = @(
    '/m',
    '/nologo',
    '/restore:false',
    '/t:Rebuild',
    '/p:Configuration=Debug',
    '/p:Platform=x64',
    "/p:WindowsTargetPlatformVersion=$kitVersion",
    '/p:SignMode=Off',
    '/verbosity:minimal'
)

& $preflight.MSBuildPath $commonProject @arguments
if ($LASTEXITCODE -ne 0) {
    throw "EndpointsCommon validation rebuild failed with exit code $LASTEXITCODE."
}

& $preflight.MSBuildPath $driverProject @arguments
if ($LASTEXITCODE -ne 0) {
    throw "MiniAEC validation driver rebuild failed with exit code $LASTEXITCODE."
}

$buildRoot = Join-Path $sysvadRoot 'TabletAudioSample\x64\Debug'
$builtInf = Join-Path $buildRoot 'MiniAECValidation.inf'
$builtSys = Join-Path $buildRoot 'TabletAudioSample.sys'
foreach ($artifact in @($builtInf, $builtSys)) {
    if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
        throw "Expected validation artifact is missing: $artifact"
    }
}

$infText = Get-Content -LiteralPath $builtInf -Raw
$captureInterfaces = [regex]::Matches($infText, '(?im)^\s*AddInterface\s*=\s*%KSCATEGORY_CAPTURE%')
if ($infText -notmatch 'MiniAEC Microphone' -or $captureInterfaces.Count -ne 1) {
    throw 'Validation INF does not declare the required MiniAEC Microphone capture endpoint.'
}
if ($infText -match 'KSCATEGORY_RENDER') {
    throw 'Validation INF unexpectedly declares a producer-facing render endpoint.'
}
if ($infText -match 'PKEY_AudioDevice_NeverSetAsDefaultEndpoint') {
    throw 'Validation INF prevents MiniAEC Microphone from being selected as a default input device.'
}

New-Item -ItemType Directory -Path $packageRoot | Out-Null
Copy-Item -LiteralPath $builtInf -Destination $packageRoot
Copy-Item -LiteralPath $builtSys -Destination $packageRoot

$inf2CatCandidates = @(
    (Join-Path $preflight.KitsRoot10 "bin\$kitVersion\x86\Inf2Cat.exe"),
    (Join-Path $preflight.KitsRoot10 "bin\$kitVersion\x64\Inf2Cat.exe")
)
$inf2Cat = $inf2CatCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $inf2Cat) {
    throw 'Inf2Cat.exe was not found in the selected WDK. The unsigned package cannot be prepared for test signing.'
}

& $inf2Cat "/driver:$packageRoot" '/os:10_X64' '/uselocaltime'
if ($LASTEXITCODE -ne 0) {
    throw "Inf2Cat failed with exit code $LASTEXITCODE."
}

$catalog = Join-Path $packageRoot 'MiniAECValidation.cat'
if (-not (Test-Path -LiteralPath $catalog -PathType Leaf)) {
    throw 'Inf2Cat completed without producing MiniAECValidation.cat.'
}

Write-Host "Unsigned x64 Debug validation package prepared at $packageRoot"
Write-Host 'The package contains only the capture-only INF, driver binary, and unsigned catalog. No certificate or driver was installed.'
