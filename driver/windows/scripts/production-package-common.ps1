Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ProductionSysVadRepository = 'https://github.com/microsoft/Windows-driver-samples'
$script:ProductionSysVadPath = 'audio/sysvad'
$script:ProductionSysVadCommit = '2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89'
$script:ProductionSysVadLicense = 'MS-PL'
$script:ProductionSysVadRecordPath = 'driver/windows/UPSTREAM.md'
$script:ProductionEvidenceSchemaVersion = 1
$script:ProductionLocalPatchPaths = @(
    'adapter.cpp',
    'EndpointsCommon/minwavertstream.cpp',
    'TabletAudioSample/micinwavtable.h',
    'TabletAudioSample/minipairs.h',
    'TabletAudioSample/TabletAudioSample.vcxproj'
)

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-TextSha256 {
    param([Parameter(Mandatory)][string]$Text)

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        return (($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join '')
    } finally {
        $sha.Dispose()
    }
}

function ConvertTo-RelativePackagePath {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$Path
    )

    return [System.IO.Path]::GetRelativePath($Root, $Path).Replace('\', '/')
}

function Get-CanonicalSourceFiles {
    param([Parameter(Mandatory)][string]$Root)

    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\')
    $records = @()
    foreach ($file in Get-ChildItem -LiteralPath $resolvedRoot -Recurse -File | Sort-Object FullName) {
        $relative = ConvertTo-RelativePackagePath -Root $resolvedRoot -Path $file.FullName
        if ($relative -match '(^|/)(x64|ARM64|Debug|Release|out|\.git)(/|$)') {
            continue
        }
        $records += [pscustomobject]@{
            path = $relative
            sha256 = Get-Sha256 -Path $file.FullName
            size_bytes = [int64]$file.Length
        }
    }
    return @($records | Sort-Object path)
}

function Get-CanonicalTreeDigest {
    param([Parameter(Mandatory)][string]$Root)

    $lines = @(
        Get-CanonicalSourceFiles -Root $Root | ForEach-Object {
            "$($_.path)`0$($_.sha256)`0$($_.size_bytes)`n"
        }
    )
    return Get-TextSha256 -Text ([string]::Concat($lines))
}

function Get-ImportedFileSetDigest {
    param([Parameter(Mandatory)][string]$Root)

    $lines = @(
        Get-CanonicalSourceFiles -Root $Root | ForEach-Object {
            "$($_.path)`n"
        }
    )
    return Get-TextSha256 -Text ([string]::Concat($lines))
}

function Get-LocalPatchEvidence {
    param([Parameter(Mandatory)][string]$DriverRoot)

    $vendorRoot = Join-Path $DriverRoot 'vendor\sysvad'
    $patches = @()
    foreach ($relative in $script:ProductionLocalPatchPaths) {
        $path = Join-Path $vendorRoot ($relative.Replace('/', '\'))
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Recorded SysVAD local patch is missing: $path"
        }
        $patches += [ordered]@{
            path = $relative
            sha256 = Get-Sha256 -Path $path
        }
    }
    return @($patches)
}

function Get-PackageFileEvidence {
    param(
        [Parameter(Mandatory)][string]$PackageRoot,
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Kind
    )

    $fullPath = Join-Path $PackageRoot ($Path.Replace('/', '\'))
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
        throw "Required production package file is missing: $fullPath"
    }
    $file = Get-Item -LiteralPath $fullPath
    return [ordered]@{
        path = $Path
        kind = $Kind
        sha256 = Get-Sha256 -Path $fullPath
        size_bytes = [int64]$file.Length
    }
}

function Get-PackageFileEvidenceSet {
    param(
        [Parameter(Mandatory)][string]$PackageRoot,
        [Parameter(Mandatory)]$Manifest
    )

    return @(
        Get-PackageFileEvidence -PackageRoot $PackageRoot -Path 'manifest.json' -Kind 'manifest'
        Get-PackageFileEvidence -PackageRoot $PackageRoot -Path $Manifest.files.runtime_executable -Kind 'runtime'
        Get-PackageFileEvidence -PackageRoot $PackageRoot -Path $Manifest.files.driver_inf -Kind 'driver-inf'
        Get-PackageFileEvidence -PackageRoot $PackageRoot -Path $Manifest.files.driver_binary -Kind 'driver-sys'
        Get-PackageFileEvidence -PackageRoot $PackageRoot -Path $Manifest.files.driver_catalog -Kind 'driver-cat'
        Get-PackageFileEvidence -PackageRoot $PackageRoot -Path 'trust/SysVAD-MS-PL.txt' -Kind 'license-notice'
    )
}

function Get-RecordByPath {
    param(
        [Parameter(Mandatory)][object[]]$Records,
        [Parameter(Mandatory)][string]$Path
    )

    $record = @($Records | Where-Object { $_.path -eq $Path }) | Select-Object -First 1
    if (-not $record) {
        throw "Package evidence has no file record for $Path"
    }
    return $record
}

function Get-CanonicalPayloadDigest {
    param(
        [Parameter(Mandatory)][object[]]$Records,
        [Parameter(Mandatory)][string]$InfPath,
        [Parameter(Mandatory)][string]$SysPath
    )

    $lines = @(
        @(
            Get-RecordByPath -Records $Records -Path $InfPath
            Get-RecordByPath -Records $Records -Path $SysPath
        ) | Sort-Object path | ForEach-Object {
            "$($_.path)`0$($_.sha256.ToLowerInvariant())`0$($_.size_bytes)`n"
        }
    )
    return Get-TextSha256 -Text ([string]::Concat($lines))
}

function Get-CanonicalCatalogMemberDigest {
    param([Parameter(Mandatory)]$Members)

    if ($Members -is [System.Collections.IDictionary]) {
        $entries = @($Members.GetEnumerator() | Sort-Object Key)
        $lines = @($entries | ForEach-Object { "$($_.Key)`0$($_.Value.ToLowerInvariant())`n" })
    } else {
        $entries = @($Members.PSObject.Properties | Sort-Object Name)
        $lines = @($entries | ForEach-Object { "$($_.Name)`0$($_.Value.ToLowerInvariant())`n" })
    }
    return Get-TextSha256 -Text ([string]::Concat($lines))
}

function Write-JsonDocument {
    param(
        [Parameter(Mandatory)]$Value,
        [Parameter(Mandatory)][string]$Path
    )

    $json = $Value | ConvertTo-Json -Depth 20
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $json, $utf8)
}

function Read-JsonDocument {
    param([Parameter(Mandatory)][string]$Path)

    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Get-ToolVersionOrUnknown {
    param([Parameter(Mandatory)]$Preflight, [Parameter(Mandatory)][string]$Property)

    $value = $Preflight.$Property
    if ([string]::IsNullOrWhiteSpace([string]$value)) {
        return 'unknown'
    }
    return [string]$value
}

function New-BuildEvidence {
    param(
        [Parameter(Mandatory)]$Preflight,
        [Parameter(Mandatory)][string[]]$Commands
    )

    $kitVersion = @($Preflight.WindowsDriverKitVersions) |
        Where-Object { $_ -in @($Preflight.WindowsSdkVersions) } |
        Sort-Object { [version]$_ } -Descending |
        Select-Object -First 1
    if (-not $kitVersion) {
        throw 'No matching Windows SDK/WDK version was reported by preflight.'
    }
    return [ordered]@{
        windows = [string]$Preflight.Windows
        visual_studio = [string]$Preflight.VisualStudio
        msbuild_version = Get-ToolVersionOrUnknown -Preflight $Preflight -Property 'MSBuildVersion'
        msvc_version = Get-ToolVersionOrUnknown -Preflight $Preflight -Property 'CppCompilerVersion'
        sdk_version = [string]$kitVersion
        wdk_version = [string]$kitVersion
        inf2cat_version = [string]$kitVersion
        signtool_version = Get-ToolVersionOrUnknown -Preflight $Preflight -Property 'SignToolVersion'
        configuration = 'Release'
        platform = 'x64'
        commands = @($Commands)
        canonicalization = 'MiniAEC package canonical v1: UTF-8 path, SHA-256, size records with detached signature bytes excluded'
    }
}

function New-SourceEvidence {
    param([Parameter(Mandatory)][string]$DriverRoot)

    $vendorRoot = Join-Path $DriverRoot 'vendor\sysvad'
    return [ordered]@{
        repository = $script:ProductionSysVadRepository
        path = $script:ProductionSysVadPath
        commit = $script:ProductionSysVadCommit
        license = $script:ProductionSysVadLicense
        record_path = $script:ProductionSysVadRecordPath
        source_tree_sha256 = Get-CanonicalTreeDigest -Root $vendorRoot
        imported_file_set_sha256 = Get-ImportedFileSetDigest -Root $vendorRoot
        local_patches = @(Get-LocalPatchEvidence -DriverRoot $DriverRoot)
    }
}

function New-PackageEvidence {
    param(
        [Parameter(Mandatory)][string]$PackageRoot,
        [Parameter(Mandatory)][string]$DriverRoot,
        [Parameter(Mandatory)]$Manifest,
        [Parameter(Mandatory)]$Build,
        [Parameter(Mandatory)]$Signing,
        [Parameter(Mandatory)][bool]$ReplayVerified,
        [Parameter(Mandatory)][bool]$VerificationVerified,
        [Parameter(Mandatory)][string]$VerificationTool,
        [Parameter(Mandatory)][string[]]$VerificationCommands
    )

    $files = @(Get-PackageFileEvidenceSet -PackageRoot $PackageRoot -Manifest $Manifest)
    $members = [ordered]@{}
    $members[$Manifest.files.driver_inf] = (Get-RecordByPath -Records $files -Path $Manifest.files.driver_inf).sha256
    $members[$Manifest.files.driver_binary] = (Get-RecordByPath -Records $files -Path $Manifest.files.driver_binary).sha256
    return [ordered]@{
        schema_version = $script:ProductionEvidenceSchemaVersion
        package_identifier = 'mini-aec'
        package_version = [string]$Manifest.release_version
        source = New-SourceEvidence -DriverRoot $DriverRoot
        build = $Build
        files = $files
        catalog = [ordered]@{
            path = [string]$Manifest.files.driver_catalog
            members = $members
            coverage_verified = [bool]$Signing.inf_catalog_coverage_verified -and [bool]$Signing.sys_catalog_coverage_verified
        }
        reproducibility = [ordered]@{
            canonical_payload_sha256 = Get-CanonicalPayloadDigest -Records $files -InfPath $Manifest.files.driver_inf -SysPath $Manifest.files.driver_binary
            catalog_member_set_sha256 = Get-CanonicalCatalogMemberDigest -Members $members
            replay_verified = $ReplayVerified
            signature_bytes_excluded = $true
        }
        signing = $Signing
        verification = [ordered]@{
            verified = $VerificationVerified
            replay_verified = $ReplayVerified
            read_only = $true
            tool = $VerificationTool
            commands = @($VerificationCommands)
        }
        privacy = [ordered]@{
            private_signing_material_present = $false
            audio_content_present = $false
            artifacts_content_present = $false
            machine_secret_material_present = $false
        }
    }
}

function Assert-PackagePathSafety {
    param([Parameter(Mandatory)][string]$PackageRoot)

    foreach ($path in Get-ChildItem -LiteralPath $PackageRoot -Recurse -File) {
        $relative = ConvertTo-RelativePackagePath -Root $PackageRoot -Path $path.FullName
        if ($relative -match '(^|/)artifacts(/|$)') {
            throw "Production package cannot contain artifacts content: $relative"
        }
        if ($path.Extension.ToLowerInvariant() -in @('.pfx', '.p12', '.pvk', '.key', '.snk', '.wav', '.flac', '.mp3', '.m4a', '.pcm')) {
            throw "Production package cannot contain private signing or audio content: $relative"
        }
    }
}
