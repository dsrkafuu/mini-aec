[CmdletBinding()]
param(
  [string]$Version,
  [string]$BinaryPath = "target/release/mini-aec.exe",
  [string]$InstallerPath,
  [string]$OutputRoot = "dist/windows-x64"
)

$ErrorActionPreference = "Stop"

function Resolve-RepositoryPath {
  param([Parameter(Mandatory)][string]$Path)

  $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
  if ([System.IO.Path]::IsPathRooted($Path)) {
    return [System.IO.Path]::GetFullPath($Path)
  }

  return [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $Path))
}

function Get-CargoVersion {
  param([Parameter(Mandatory)][string]$Path)

  $versionLine = Get-Content -LiteralPath $Path |
    Where-Object { $_ -match '^\s*version\s*=\s*"([^"]+)"' } |
    Select-Object -First 1
  if ($null -eq $versionLine -or $versionLine -notmatch '"([^"]+)"') {
    throw "Could not read a package version from $Path."
  }

  return $Matches[1]
}

function Write-Utf8NoBom {
  param(
    [Parameter(Mandatory)][string]$Path,
    [Parameter(Mandatory)][string]$Text
  )

  [System.IO.File]::WriteAllText(
    $Path,
    $Text,
    [System.Text.UTF8Encoding]::new($false)
  )
}

function Get-CommandSummary {
  param([Parameter(Mandatory)][string]$Command)

  try {
    $result = & $Command --version 2>$null | Select-Object -First 1
    if ($result) {
      return $result.ToString().Trim()
    }
  } catch {
    return "unavailable"
  }

  return "unavailable"
}

function Get-SourceMetadata {
  param([Parameter(Mandatory)][string]$RepositoryRoot)

  $revision = (& git -C $RepositoryRoot rev-parse HEAD 2>$null | Out-String).Trim()
  if ([string]::IsNullOrWhiteSpace($revision)) {
    $revision = "unavailable"
  }

  $status = (& git -C $RepositoryRoot status --porcelain 2>$null | Out-String).Trim()
  return [ordered]@{
    revision = $revision
    workingTreeDirty = -not [string]::IsNullOrWhiteSpace($status)
  }
}

function Get-SignatureRecord {
  param([Parameter(Mandatory)][string]$Path)

  $signature = Get-AuthenticodeSignature -LiteralPath $Path
  $status = switch ($signature.Status.ToString()) {
    "Valid" { "valid"; break }
    "NotSigned" { "unsigned"; break }
    default { "invalid" }
  }

  $signer = $null
  $thumbprint = $null
  if ($null -ne $signature.SignerCertificate) {
    $signer = $signature.SignerCertificate.Subject
    $thumbprint = $signature.SignerCertificate.Thumbprint
  }

  return [ordered]@{
    path = [System.IO.Path]::GetFileName($Path)
    state = $status
    signer = $signer
    thumbprint = $thumbprint
  }
}

function Get-RelativeFileName {
  param(
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][string]$Path
  )

  $rootUri = [System.Uri]::new($Root.TrimEnd("\") + "\")
  $fileUri = [System.Uri]::new($Path)
  return [System.Uri]::UnescapeDataString(
    $rootUri.MakeRelativeUri($fileUri).ToString()
  ).Replace("\", "/")
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$rootCargoPath = Join-Path $repositoryRoot "Cargo.toml"
$tauriCargoPath = Join-Path $repositoryRoot "src-tauri/Cargo.toml"
$tauriConfigPath = Join-Path $repositoryRoot "src-tauri/tauri.conf.json"
$resolvedBinaryPath = Resolve-RepositoryPath $BinaryPath

$workspaceVersion = Get-CargoVersion $rootCargoPath
$tauriVersion = Get-CargoVersion $tauriCargoPath
$tauriConfig = Get-Content -Raw -LiteralPath $tauriConfigPath | ConvertFrom-Json
$configVersion = $tauriConfig.version
if ([string]::IsNullOrWhiteSpace($Version)) {
  $Version = $workspaceVersion
}

if ($workspaceVersion -ne $tauriVersion -or $workspaceVersion -ne $configVersion -or $workspaceVersion -ne $Version) {
  throw "Cargo workspace, src-tauri Cargo.toml, Tauri config, and release version must match."
}

$normalizedOutputRoot = $OutputRoot.Replace("/", "\")
if ($normalizedOutputRoot -match '(^|\\)(target|artifacts)(\\|$)') {
  throw "Release staging must not be placed under target or artifacts."
}

$resolvedOutputRoot = Resolve-RepositoryPath $OutputRoot
$stagePath = Join-Path $resolvedOutputRoot $Version
if (Test-Path -LiteralPath $stagePath) {
  Remove-Item -LiteralPath $stagePath -Recurse -Force
}
New-Item -ItemType Directory -Path $stagePath -Force | Out-Null

if (-not (Test-Path -LiteralPath $resolvedBinaryPath -PathType Leaf)) {
  throw "Release executable not found: $resolvedBinaryPath"
}

if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
  $bundleRoots = @(
    (Join-Path $repositoryRoot "target/release/bundle/nsis"),
    (Join-Path $repositoryRoot "src-tauri/target/release/bundle/nsis")
  )
  $installerCandidates = @(
    foreach ($bundleRoot in $bundleRoots) {
      if (Test-Path -LiteralPath $bundleRoot -PathType Container) {
        Get-ChildItem -LiteralPath $bundleRoot -Filter "*.exe" -File
      }
    }
  )
  if ($installerCandidates.Count -ne 1) {
    throw "Pass -InstallerPath when the Tauri NSIS output is absent or ambiguous."
  }
  $InstallerPath = $installerCandidates[0].FullName
} else {
  $InstallerPath = Resolve-RepositoryPath $InstallerPath
}

if (-not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) {
  throw "NSIS installer not found: $InstallerPath"
}

$stagedBinaryPath = Join-Path $stagePath "MiniAEC.exe"
$stagedInstallerPath = Join-Path $stagePath "MiniAEC_${Version}_x64-setup.exe"
Copy-Item -LiteralPath $resolvedBinaryPath -Destination $stagedBinaryPath
Copy-Item -LiteralPath $InstallerPath -Destination $stagedInstallerPath
Copy-Item -LiteralPath (Join-Path $repositoryRoot "README.md") -Destination (Join-Path $stagePath "README.md")
Copy-Item -LiteralPath (Join-Path $repositoryRoot "README.zh.md") -Destination (Join-Path $stagePath "README.zh.md")

$signatureRecords = @(
  Get-SignatureRecord $stagedBinaryPath
  Get-SignatureRecord $stagedInstallerPath
)
$signatureStates = @($signatureRecords | ForEach-Object { $_.state } | Select-Object -Unique)
$signatureState = if ($signatureStates.Count -eq 1) {
  $signatureStates[0]
} elseif ($signatureStates -contains "invalid") {
  "invalid"
} else {
  "mixed"
}

$sourceMetadata = Get-SourceMetadata $repositoryRoot
$rustcVerbose = (& rustc -vV 2>$null | Out-String).Trim()
$toolchain = [ordered]@{
  rustc = Get-CommandSummary "rustc"
  cargo = Get-CommandSummary "cargo"
  host = (($rustcVerbose -split "`r?`n" | Where-Object { $_ -like "host:*" } | Select-Object -First 1) -replace '^host:\s*', '')
}

$payloadFiles = Get-ChildItem -LiteralPath $stagePath -File | Sort-Object Name
$fileRecords = foreach ($file in $payloadFiles) {
  $relative = Get-RelativeFileName -Root $stagePath -Path $file.FullName
  [ordered]@{
    path = $relative
    bytes = $file.Length
    sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  }
}

$manifest = [ordered]@{
  schemaVersion = 1
  product = "MiniAEC"
  version = $Version
  platform = "Windows 11"
  architecture = "x64"
  source = $sourceMetadata
  build = [ordered]@{
    builtAtUtc = (Get-Date).ToUniversalTime().ToString("o")
    toolchain = $toolchain
  }
  signature = [ordered]@{
    state = $signatureState
    files = $signatureRecords
    note = if ($signatureState -eq "unsigned") {
      "No production code-signing certificate was applied to this build."
    } else {
      "Signature state is recorded from the packaged executable and installer."
    }
  }
  prerequisites = [ordered]@{
    vbCable = "external"
    vbCableIncluded = $false
    vbCableManagedByMiniAec = $false
    rebootRequested = $false
  }
  exclusions = @(
    "target/",
    "artifacts/",
    "*.pdb",
    "*.inf",
    "*.sys",
    "*.cat",
    "VB-CABLE installer and driver materials"
  )
  files = $fileRecords
  verification = [ordered]@{
    sha256File = "SHA256SUMS.txt"
    command = "tools/verify-windows-release.ps1"
  }
}

$manifestPath = Join-Path $stagePath "release-manifest.json"
$manifestJson = $manifest | ConvertTo-Json -Depth 10
Write-Utf8NoBom -Path $manifestPath -Text ($manifestJson + "`r`n")

$checksumFiles = Get-ChildItem -LiteralPath $stagePath -File |
  Where-Object { $_.Name -ne "SHA256SUMS.txt" } |
  Sort-Object Name
$checksumLines = foreach ($file in $checksumFiles) {
  $relative = Get-RelativeFileName -Root $stagePath -Path $file.FullName
  $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  "$hash *$relative"
}
$checksumPath = Join-Path $stagePath "SHA256SUMS.txt"
Write-Utf8NoBom -Path $checksumPath -Text (($checksumLines -join "`r`n") + "`r`n")

$forbiddenFiles = Get-ChildItem -LiteralPath $stagePath -Recurse -File |
  ForEach-Object { Get-RelativeFileName -Root $stagePath -Path $_.FullName } |
  Where-Object {
    $_ -match '(?i)(^|/)(target|artifacts)(/|$)' -or
    $_ -match '(?i)\.(pdb|inf|sys|cat)$' -or
    $_ -match '(?i)(vb[-_]?cable|vbaudio)'
  }
if ($forbiddenFiles) {
  throw "Forbidden release files found: $($forbiddenFiles -join ", ")"
}

Write-Output "Created Windows x64 release staging: $stagePath"
Write-Output "Version: $Version"
Write-Output "Source revision: $($sourceMetadata.revision)"
Write-Output "Signature state: $signatureState"
Write-Output "Files: $((Get-ChildItem -LiteralPath $stagePath -File).Count)"
