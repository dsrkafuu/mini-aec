[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$sysvadRoot = Join-Path $driverRoot 'vendor\sysvad'
$preflightScript = Join-Path $scriptRoot 'preflight.ps1'

$preflightJson = & $preflightScript -Json
if ($LASTEXITCODE -ne 0) {
    throw 'Driver toolchain preflight failed. Install the reported prerequisites before retrying the baseline build.'
}
$preflight = $preflightJson | ConvertFrom-Json

$commonProject = Join-Path $sysvadRoot 'EndpointsCommon\EndpointsCommon.vcxproj'
$driverProject = Join-Path $sysvadRoot 'TabletAudioSample\TabletAudioSample.vcxproj'
foreach ($project in @($commonProject, $driverProject)) {
    if (-not (Test-Path -LiteralPath $project -PathType Leaf)) {
        throw "Pinned SysVAD project is missing: $project"
    }
}

$arguments = @(
    '/m',
    '/nologo',
    '/restore:false',
    '/p:Configuration=Debug',
    '/p:Platform=x64',
    '/p:SignMode=Off',
    '/verbosity:minimal'
)

& $preflight.MSBuildPath $commonProject @arguments
if ($LASTEXITCODE -ne 0) {
    throw "EndpointsCommon baseline build failed with exit code $LASTEXITCODE."
}

& $preflight.MSBuildPath $driverProject @arguments
if ($LASTEXITCODE -ne 0) {
    throw "TabletAudioSample baseline build failed with exit code $LASTEXITCODE."
}

Write-Host 'Unsigned x64 Debug SysVAD baseline build completed. No certificate or driver was installed.'
