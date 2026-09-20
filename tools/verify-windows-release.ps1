[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$PackageRoot
)

$ErrorActionPreference = "Stop"

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

function Get-SignatureState {
  param([Parameter(Mandatory)][string]$Path)

  $signature = Get-AuthenticodeSignature -LiteralPath $Path
  switch ($signature.Status.ToString()) {
    "Valid" { return "valid" }
    "NotSigned" { return "unsigned" }
    default { return "invalid" }
  }
}

$resolvedPackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
$manifestPath = Join-Path $resolvedPackageRoot "release-manifest.json"
$checksumPath = Join-Path $resolvedPackageRoot "SHA256SUMS.txt"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "Release manifest not found: $manifestPath"
}
if (-not (Test-Path -LiteralPath $checksumPath -PathType Leaf)) {
  throw "SHA-256 file not found: $checksumPath"
}

$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.product -ne "MiniAEC") {
  throw "Unexpected product in release manifest."
}
if ($manifest.platform -ne "Windows 11" -or $manifest.architecture -ne "x64") {
  throw "Release manifest is not for Windows 11 x64."
}
if ($manifest.prerequisites.vbCableIncluded -ne $false -or
  $manifest.prerequisites.vbCableManagedByMiniAec -ne $false -or
  $manifest.prerequisites.rebootRequested -ne $false) {
  throw "Release manifest violates the external VB-CABLE boundary."
}

$allFiles = Get-ChildItem -LiteralPath $resolvedPackageRoot -Recurse -File
$relativeFiles = @($allFiles | ForEach-Object {
  Get-RelativeFileName -Root $resolvedPackageRoot -Path $_.FullName
})
$expectedFileSet = [System.Collections.Generic.HashSet[string]]::new(
  [System.StringComparer]::OrdinalIgnoreCase
)
$expectedFileSet.Add("release-manifest.json") | Out-Null
$expectedFileSet.Add("SHA256SUMS.txt") | Out-Null
foreach ($entry in @($manifest.files)) {
  $expectedFileSet.Add($entry.path.Replace("\", "/")) | Out-Null
}
$unexpectedFiles = @($relativeFiles | Where-Object { -not $expectedFileSet.Contains($_) })
if ($unexpectedFiles.Count -gt 0) {
  throw "Unexpected files found in release package: $($unexpectedFiles -join ", ")"
}
$missingFiles = @($expectedFileSet | Where-Object { $_ -notin $relativeFiles })
if ($missingFiles.Count -gt 0) {
  throw "Manifest or checksum references missing files: $($missingFiles -join ", ")"
}
$forbiddenFiles = @($relativeFiles | Where-Object {
  $_ -match '(?i)(^|/)(target|artifacts)(/|$)' -or
  $_ -match '(?i)\.(pdb|inf|sys|cat)$' -or
  $_ -match '(?i)(vb[-_]?cable|vbaudio)'
})
if ($forbiddenFiles.Count -gt 0) {
  throw "Forbidden release files found: $($forbiddenFiles -join ", ")"
}

$checksumLines = Get-Content -LiteralPath $checksumPath |
  Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
$checksumRelativePaths = [System.Collections.Generic.HashSet[string]]::new(
  [System.StringComparer]::OrdinalIgnoreCase
)
foreach ($line in $checksumLines) {
  $parts = $line -split '\s+', 2
  if ($parts.Count -ne 2) {
    throw "Invalid checksum line: $line"
  }

  $expectedHash = $parts[0].ToLowerInvariant()
  $relative = $parts[1].TrimStart("*").Replace("\", "/")
  if (-not $checksumRelativePaths.Add($relative)) {
    throw "Duplicate checksum entry: $relative"
  }
  $filePath = Join-Path $resolvedPackageRoot ($relative.Replace("/", "\"))
  if (-not (Test-Path -LiteralPath $filePath -PathType Leaf)) {
    throw "Checksum references missing file: $relative"
  }

  $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualHash -ne $expectedHash) {
    throw "SHA-256 mismatch: $relative"
  }
}
$expectedChecksumPaths = @($relativeFiles | Where-Object { $_ -ne "SHA256SUMS.txt" })
$missingChecksumPaths = @($expectedChecksumPaths | Where-Object {
  -not $checksumRelativePaths.Contains($_)
})
if ($missingChecksumPaths.Count -gt 0) {
  throw "Checksum file does not cover: $($missingChecksumPaths -join ", ")"
}
$unexpectedChecksumPaths = @($checksumRelativePaths | Where-Object {
  $_ -eq "SHA256SUMS.txt" -or $_ -notin $expectedChecksumPaths
})
if ($unexpectedChecksumPaths.Count -gt 0) {
  throw "Checksum file contains unexpected paths: $($unexpectedChecksumPaths -join ", ")"
}

foreach ($entry in @($manifest.files)) {
  $relative = $entry.path.Replace("\", "/")
  $filePath = Join-Path $resolvedPackageRoot ($relative.Replace("/", "\"))
  if (-not (Test-Path -LiteralPath $filePath -PathType Leaf)) {
    throw "Manifest references missing file: $relative"
  }

  $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualHash -ne $entry.sha256.ToLowerInvariant()) {
    throw "Manifest SHA-256 mismatch: $relative"
  }
}

$signatureStates = foreach ($entry in @($manifest.signature.files)) {
  $filePath = Join-Path $resolvedPackageRoot $entry.path
  $actualState = Get-SignatureState $filePath
  if ($actualState -ne $entry.state) {
    throw "Signature state changed for $($entry.path): manifest=$($entry.state), actual=$actualState"
  }
  $actualState
}
$uniqueStates = @($signatureStates | Select-Object -Unique)
$expectedOverallState = if ($uniqueStates.Count -eq 1) {
  $uniqueStates[0]
} elseif ($uniqueStates -contains "invalid") {
  "invalid"
} else {
  "mixed"
}
if ($manifest.signature.state -ne $expectedOverallState) {
  throw "Overall signature state mismatch."
}

Write-Output "Release verification passed: $resolvedPackageRoot"
Write-Output "Version: $($manifest.version)"
Write-Output "Architecture: $($manifest.architecture)"
Write-Output "Signature state: $($manifest.signature.state)"
Write-Output "Checksums verified: $($checksumLines.Count)"
