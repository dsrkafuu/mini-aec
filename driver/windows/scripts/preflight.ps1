[CmdletBinding()]
param(
    [switch]$Json
)

$ErrorActionPreference = "Stop"

function Get-ExistingFileVersion {
    param([string]$Path)

    if (-not $Path -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }

    return (Get-Item -LiteralPath $Path).VersionInfo.ProductVersion
}

function Get-VersionDirectories {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        return @()
    }

    return @(Get-ChildItem -LiteralPath $Path -Directory | Where-Object { $_.Name -match '^10\.0\.\d+\.\d+$' } | Sort-Object { [version]$_.Name } | ForEach-Object { $_.Name })
}

$windowsKey = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$windowsProductName = $windowsKey.ProductName
if ([int]$windowsKey.CurrentBuildNumber -ge 22000 -and $windowsProductName -like 'Windows 10*') {
    $windowsProductName = $windowsProductName -replace '^Windows 10', 'Windows 11'
}
$windowsVersion = "{0} {1} build {2}.{3}" -f $windowsProductName, $windowsKey.DisplayVersion, $windowsKey.CurrentBuildNumber, $windowsKey.UBR

$vswhereCandidates = @(
    (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'),
    (Join-Path $env:ProgramFiles 'Microsoft Visual Studio\Installer\vswhere.exe')
)
$vswhere = $vswhereCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
$visualStudio = $null
$visualStudioPath = $null
$msbuildPath = $null

if ($vswhere) {
    $installationJson = & $vswhere -latest -products '*' -requires Microsoft.Component.MSBuild -format json -utf8
    $installation = @($installationJson | ConvertFrom-Json) | Select-Object -First 1
    if ($installation) {
        $visualStudio = "{0} ({1})" -f $installation.displayName, $installation.installationVersion
        $visualStudioPath = $installation.installationPath
        $candidateMsbuild = Join-Path $installation.installationPath 'MSBuild\Current\Bin\MSBuild.exe'
        if (Test-Path -LiteralPath $candidateMsbuild -PathType Leaf) {
            $msbuildPath = $candidateMsbuild
        }
    }
}

$msvcVersions = @()
$cppCompilerPath = $null
$spectreLibrariesPath = $null
if ($visualStudioPath) {
    $msvcRoot = Join-Path $visualStudioPath 'VC\Tools\MSVC'
    if (Test-Path -LiteralPath $msvcRoot -PathType Container) {
        $msvcVersions = @(Get-ChildItem -LiteralPath $msvcRoot -Directory | Sort-Object { [version]$_.Name } | ForEach-Object { $_.Name })
        foreach ($version in @($msvcVersions | Sort-Object { [version]$_ } -Descending)) {
            $candidateCompiler = Join-Path $msvcRoot "$version\bin\Hostx64\x64\cl.exe"
            if (-not $cppCompilerPath -and (Test-Path -LiteralPath $candidateCompiler -PathType Leaf)) {
                $cppCompilerPath = $candidateCompiler
            }
            $candidateSpectreLibraries = Join-Path $msvcRoot "$version\lib\spectre\x64"
            if (-not $spectreLibrariesPath -and (Test-Path -LiteralPath $candidateSpectreLibraries -PathType Container)) {
                $spectreLibrariesPath = $candidateSpectreLibraries
            }
        }
    }
}

$kitsRoot = $null
foreach ($registryPath in @('HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots')) {
    if (Test-Path -LiteralPath $registryPath) {
        $candidateRoot = (Get-ItemProperty -LiteralPath $registryPath -Name KitsRoot10 -ErrorAction SilentlyContinue).KitsRoot10
        if ($candidateRoot) {
            $kitsRoot = $candidateRoot
            break
        }
    }
}

$sdkVersions = @()
$wdkVersions = @()
$signToolPath = $null
if ($kitsRoot) {
    $sdkVersions = Get-VersionDirectories -Path (Join-Path $kitsRoot 'Include')
    $buildRoot = Join-Path $kitsRoot 'build'
    $wdkVersions = @(Get-VersionDirectories -Path $buildRoot | Where-Object { Test-Path -LiteralPath (Join-Path $buildRoot "$_\WindowsDriver.Common.targets") -PathType Leaf })
    foreach ($version in @($wdkVersions | Sort-Object { [version]$_ } -Descending)) {
        $candidateSignTool = Join-Path $kitsRoot "bin\$version\x64\signtool.exe"
        if (Test-Path -LiteralPath $candidateSignTool -PathType Leaf) {
            $signToolPath = $candidateSignTool
            break
        }
    }
}

$result = [ordered]@{
    Windows = $windowsVersion
    VisualStudio = $visualStudio
    VsWhere = $vswhere
    MSBuildPath = $msbuildPath
    MSBuildVersion = Get-ExistingFileVersion -Path $msbuildPath
    MsvcVersions = $msvcVersions
    CppCompilerPath = $cppCompilerPath
    CppCompilerVersion = Get-ExistingFileVersion -Path $cppCompilerPath
    SpectreLibrariesPath = $spectreLibrariesPath
    KitsRoot10 = $kitsRoot
    WindowsSdkVersions = $sdkVersions
    WindowsDriverKitVersions = $wdkVersions
    SignToolPath = $signToolPath
    SignToolVersion = Get-ExistingFileVersion -Path $signToolPath
}

if ($Json) {
    [pscustomobject]$result | ConvertTo-Json -Depth 4
} else {
    [pscustomobject]$result | Format-List
}

$missing = @()
if (-not $visualStudio) { $missing += 'Visual Studio with MSBuild' }
if (-not $msbuildPath) { $missing += 'MSBuild.exe' }
if (-not $cppCompilerPath) { $missing += 'x64 C++ compiler' }
if (-not $spectreLibrariesPath) { $missing += 'x64 Spectre-mitigated C++ libraries' }
if ($sdkVersions.Count -eq 0) { $missing += 'Windows SDK' }
if ($wdkVersions.Count -eq 0) { $missing += 'Windows Driver Kit build targets' }
if (-not $signToolPath) { $missing += 'x64 SignTool' }

if ($missing.Count -gt 0) {
    Write-Error ("Driver build prerequisites are incomplete: {0}. Install the matching Visual Studio C++ workload, Windows SDK, and WDK before building; no system state was changed." -f ($missing -join ', '))
    exit 1
}
