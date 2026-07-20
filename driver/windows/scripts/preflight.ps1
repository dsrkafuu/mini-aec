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

function Get-VsWhereProperty {
    param(
        [string]$Path,
        [string]$Property
    )

    $values = @(& $Path -latest -products '*' -requires Microsoft.Component.MSBuild -property $Property -utf8)
    if ($LASTEXITCODE -ne 0) {
        return $null
    }
    $value = $values | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($value)) {
        return $null
    }
    return $value.Trim()
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
$msbuildArchitecture = $null

if ($vswhere) {
    $visualStudioPath = Get-VsWhereProperty -Path $vswhere -Property 'installationPath'
    $visualStudioName = Get-VsWhereProperty -Path $vswhere -Property 'displayName'
    $visualStudioVersion = Get-VsWhereProperty -Path $vswhere -Property 'installationVersion'
    if ($visualStudioPath) {
        $visualStudio = "{0} ({1})" -f $visualStudioName, $visualStudioVersion
        foreach ($candidate in @(
            @{ Path = (Join-Path $visualStudioPath 'MSBuild\Current\Bin\amd64\MSBuild.exe'); Architecture = 'x64' },
            @{ Path = (Join-Path $visualStudioPath 'MSBuild\Current\Bin\MSBuild.exe'); Architecture = 'x86' }
        )) {
            if (-not $msbuildPath -and (Test-Path -LiteralPath $candidate.Path -PathType Leaf)) {
                $msbuildPath = $candidate.Path
                $msbuildArchitecture = $candidate.Architecture
            }
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
$incompleteSdkVersions = @()
$wdkVersions = @()
$signToolPath = $null
if ($kitsRoot) {
    $sdkCandidates = Get-VersionDirectories -Path (Join-Path $kitsRoot 'Include')
    foreach ($version in $sdkCandidates) {
        $requiredSdkPaths = @(
            (Join-Path $kitsRoot "DesignTime\CommonConfiguration\Neutral\UAP\$version\UAP.props"),
            (Join-Path $kitsRoot "Include\$version\shared\sdkddkver.h"),
            (Join-Path $kitsRoot "Include\$version\um\Windows.h"),
            (Join-Path $kitsRoot "Include\$version\ucrt\stdio.h"),
            (Join-Path $kitsRoot "Lib\$version\um\x64\gdi32.lib"),
            (Join-Path $kitsRoot "Lib\$version\ucrt\x64\ucrt.lib")
        )
        if (@($requiredSdkPaths | Where-Object { -not (Test-Path -LiteralPath $_ -PathType Leaf) }).Count -eq 0) {
            $sdkVersions += $version
        } else {
            $incompleteSdkVersions += $version
        }
    }
    $buildRoot = Join-Path $kitsRoot 'build'
    $wdkVersions = @(Get-VersionDirectories -Path $buildRoot | Where-Object { Test-Path -LiteralPath (Join-Path $buildRoot "$_\WindowsDriver.Common.targets") -PathType Leaf })
    $toolVersions = @(($wdkVersions + $sdkVersions + $incompleteSdkVersions) | Sort-Object { [version]$_ } -Descending -Unique)
    foreach ($version in $toolVersions) {
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
    MSBuildArchitecture = $msbuildArchitecture
    MsvcVersions = $msvcVersions
    CppCompilerPath = $cppCompilerPath
    CppCompilerVersion = Get-ExistingFileVersion -Path $cppCompilerPath
    SpectreLibrariesPath = $spectreLibrariesPath
    KitsRoot10 = $kitsRoot
    WindowsSdkVersions = $sdkVersions
    IncompleteWindowsSdkVersions = $incompleteSdkVersions
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
if ($sdkVersions.Count -gt 0 -and $wdkVersions.Count -gt 0 -and @($sdkVersions | Where-Object { $_ -in $wdkVersions }).Count -eq 0) { $missing += 'matching Windows SDK/WDK build number' }
if (-not $signToolPath) { $missing += 'x64 SignTool' }

if ($missing.Count -gt 0) {
    Write-Error ("Driver build prerequisites are incomplete: {0}. Install the matching Visual Studio C++ workload, Windows SDK, and WDK before building; no system state was changed." -f ($missing -join ', '))
    exit 1
}
