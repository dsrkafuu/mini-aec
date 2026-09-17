[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$CheckoutRoot
)

$ErrorActionPreference = 'Stop'
$expectedCommit = '2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89'
$checkout = (Resolve-Path -LiteralPath $CheckoutRoot).Path
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$destinationRoot = Join-Path $driverRoot 'vendor\sysvad'
$sourceRoot = Join-Path $checkout 'audio\sysvad'
$allowedLocalPatches = @(
    'adapter.cpp',
    'EndpointsCommon/minwavertstream.cpp',
    'TabletAudioSample/micintopo.cpp',
    'TabletAudioSample/micintoptable.h',
    'TabletAudioSample/micinwavtable.h',
    'TabletAudioSample/minipairs.h',
    'TabletAudioSample/TabletAudioSample.vcxproj'
)

$actualCommit = (& git -C $checkout rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $actualCommit -ne $expectedCommit) {
    throw "Expected upstream commit $expectedCommit but found $actualCommit."
}

$expected = @(
    [pscustomobject]@{ Relative = 'LICENSE'; Source = Join-Path $checkout 'LICENSE' },
    [pscustomobject]@{ Relative = 'README.md'; Source = Join-Path $sourceRoot 'README.md' }
)

foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -File | Where-Object { $_.Extension -in @('.cpp', '.h') }) {
    $expected += [pscustomobject]@{ Relative = $file.Name; Source = $file.FullName }
}

foreach ($directoryName in @('EndpointsCommon', 'TabletAudioSample')) {
    $directory = Join-Path $sourceRoot $directoryName
    foreach ($file in Get-ChildItem -LiteralPath $directory -Recurse -File) {
        $relativeWithinDirectory = [System.IO.Path]::GetRelativePath($directory, $file.FullName)
        $expected += [pscustomobject]@{ Relative = Join-Path $directoryName $relativeWithinDirectory; Source = $file.FullName }
    }
}

$expectedByPath = @{}
foreach ($entry in $expected) {
    $key = $entry.Relative.Replace('\', '/')
    $expectedByPath[$key] = $entry.Source
}

$actualByPath = @{}
foreach ($file in Get-ChildItem -LiteralPath $destinationRoot -Recurse -File) {
    $relative = [System.IO.Path]::GetRelativePath($destinationRoot, $file.FullName).Replace('\', '/')
    if ($relative -match '(^|/)(x64|ARM64)(/|$)') {
        continue
    }
    $actualByPath[$relative] = $file.FullName
}

$missing = @($expectedByPath.Keys | Where-Object { -not $actualByPath.ContainsKey($_) } | Sort-Object)
$unexpected = @($actualByPath.Keys | Where-Object { -not $expectedByPath.ContainsKey($_) } | Sort-Object)
if ($missing.Count -gt 0 -or $unexpected.Count -gt 0) {
    throw "Imported file set differs from the pinned source slice. Missing: $($missing -join ', '); unexpected: $($unexpected -join ', ')."
}

$mismatched = @()
foreach ($relative in $expectedByPath.Keys) {
    $sourceHash = (Get-FileHash -LiteralPath $expectedByPath[$relative] -Algorithm SHA256).Hash
    $destinationHash = (Get-FileHash -LiteralPath $actualByPath[$relative] -Algorithm SHA256).Hash
    if ($sourceHash -ne $destinationHash) {
        $mismatched += $relative
    }
}

$unexpectedMismatches = @($mismatched | Where-Object { $_ -notin $allowedLocalPatches } | Sort-Object)
$missingPatches = @($allowedLocalPatches | Where-Object { $_ -notin $mismatched } | Sort-Object)
if ($unexpectedMismatches.Count -gt 0 -or $missingPatches.Count -gt 0) {
    throw "Imported file contents differ from the pinned source outside the patch ledger. Unexpected mismatches: $($unexpectedMismatches -join ', '); recorded patches not present: $($missingPatches -join ', ')."
}

Write-Host "Verified $($expectedByPath.Count) imported files against SysVAD commit $expectedCommit with $($allowedLocalPatches.Count) recorded local patches."
