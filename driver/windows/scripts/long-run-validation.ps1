[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$MicrophoneId,
    [Parameter(Mandatory)]
    [string]$RenderId,
    [ValidateRange(1, 86400)]
    [int]$DurationSeconds = 1800,
    [string]$EvidenceRoot = 'artifacts\long-run-audio-stability',
    [string]$AudioPath = 'driver\windows\out\validation\synthetic-far-end.wav',
    [string]$FfmpegPath = 'ffmpeg.exe',
    [string]$FfplayPath = 'ffplay.exe',
    [string]$CaptureClientDeviceName = 'MiniAEC Microphone (MiniAEC Virtual Audio Device (Development))',
    [string]$PlaybackFilter = 'loudnorm=I=-14:TP=-1:LRA=5',
    [ValidateRange(0, 100)]
    [int]$PlaybackVolume = 100,
    [string]$LabPath = 'target\release\mini-aec-lab.exe',
    [switch]$PlanOnly
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $driverRoot)
$runtimeValidationPath = Join-Path $scriptRoot 'runtime-access-validation.ps1'

function Resolve-RepositoryPath {
    param([Parameter(Mandatory)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $Path))
}

function Resolve-ExecutablePath {
    param([Parameter(Mandatory)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path) -or $Path.Contains('\') -or $Path.Contains('/')) {
        $resolved = Resolve-RepositoryPath -Path $Path
        if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
            throw "Required executable is missing: $resolved"
        }
        return $resolved
    }

    $command = Get-Command $Path -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $command) {
        throw "Required executable is not available on PATH: $Path"
    }
    return $command.Source
}

function Quote-ProcessArgument {
    param([Parameter(Mandatory)][string]$Value)

    return '"' + $Value.Replace('"', '\"') + '"'
}

function Stop-OwnedProcess {
    param([System.Diagnostics.Process]$Process)

    if ($null -ne $Process -and -not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force
        $Process.WaitForExit()
    }
}

$artifactsRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot 'artifacts')).TrimEnd('\')
$resolvedEvidenceRoot = (Resolve-RepositoryPath -Path $EvidenceRoot).TrimEnd('\')
if ($resolvedEvidenceRoot -ne $artifactsRoot -and -not $resolvedEvidenceRoot.StartsWith($artifactsRoot + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Long-run evidence must remain below the ignored artifacts root: $artifactsRoot"
}

$resolvedAudioPath = Resolve-RepositoryPath -Path $AudioPath
$resolvedLabPath = Resolve-RepositoryPath -Path $LabPath
$resolvedFfmpegPath = Resolve-ExecutablePath -Path $FfmpegPath
$resolvedFfplayPath = Resolve-ExecutablePath -Path $FfplayPath
foreach ($requiredFile in @($resolvedAudioPath, $resolvedLabPath, $runtimeValidationPath)) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        throw "Required long-run validation file is missing: $requiredFile"
    }
}

$plan = [ordered]@{
    schema_version = 1
    repository_root = $repositoryRoot
    evidence_root = $resolvedEvidenceRoot
    microphone_id_supplied = -not [string]::IsNullOrWhiteSpace($MicrophoneId)
    render_id_supplied = -not [string]::IsNullOrWhiteSpace($RenderId)
    duration_seconds = $DurationSeconds
    audio_path = $resolvedAudioPath
    playback_executable = $resolvedFfplayPath
    playback_filter = $PlaybackFilter
    playback_volume = $PlaybackVolume
    capture_client_executable = $resolvedFfmpegPath
    capture_client_device_name = $CaptureClientDeviceName
    capture_client_output = 'client-recording.flac'
    runtime_validation_script = $runtimeValidationPath
    lab_path = $resolvedLabPath
    system_changes = @()
    runtime_only = $true
    requires_non_elevated_interactive_token = $true
    creates_private_audio_recording = $true
    operator_observation_still_required = $true
}

if ($PlanOnly) {
    $plan | ConvertTo-Json -Depth 4
    return
}

$runName = '{0}-unattended-{1}s-{2}' -f [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'), $DurationSeconds, $PID
$runRoot = Join-Path $resolvedEvidenceRoot $runName
New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
$plan | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runRoot 'orchestration-plan.json') -Encoding utf8

$playbackStdout = Join-Path $runRoot 'playback.stdout.txt'
$playbackStderr = Join-Path $runRoot 'playback.stderr.txt'
$consumerStdout = Join-Path $runRoot 'capture-client.stdout.txt'
$consumerStderr = Join-Path $runRoot 'capture-client.stderr.txt'
$runtimeStdout = Join-Path $runRoot 'runtime-validation.stdout.txt'
$runtimeStderr = Join-Path $runRoot 'runtime-validation.stderr.txt'
$recordingPath = Join-Path $runRoot 'client-recording.flac'
$consumerDuration = $DurationSeconds + 8
$playback = $null
$consumer = $null
$completed = $false
$failure = $null

try {
    $consumerArguments = @(
        '-hide_banner',
        '-nostdin',
        '-loglevel', 'warning',
        '-f', 'dshow',
        '-i', (Quote-ProcessArgument -Value ('audio=' + $CaptureClientDeviceName)),
        '-t', $consumerDuration,
        '-c:a', 'flac',
        '-y', (Quote-ProcessArgument -Value $recordingPath)
    )
    $consumer = Start-Process -FilePath $resolvedFfmpegPath -ArgumentList $consumerArguments -WorkingDirectory $repositoryRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $consumerStdout -RedirectStandardError $consumerStderr

    $playbackArguments = @(
        '-nodisp',
        '-loop', '0',
        '-volume', $PlaybackVolume,
        '-af', (Quote-ProcessArgument -Value $PlaybackFilter),
        (Quote-ProcessArgument -Value $resolvedAudioPath)
    )
    $playback = Start-Process -FilePath $resolvedFfplayPath -ArgumentList $playbackArguments -WorkingDirectory $repositoryRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $playbackStdout -RedirectStandardError $playbackStderr

    Start-Sleep -Seconds 3
    if ($consumer.HasExited) {
        throw "Capture client exited before AEC start with code $($consumer.ExitCode): $(Get-Content -LiteralPath $consumerStderr -Raw -ErrorAction SilentlyContinue)"
    }
    if ($playback.HasExited) {
        throw "Playback exited before AEC start with code $($playback.ExitCode): $(Get-Content -LiteralPath $playbackStderr -Raw -ErrorAction SilentlyContinue)"
    }

    try {
        & $runtimeValidationPath -Action Aec -MicrophoneId $MicrophoneId -RenderId $RenderId -DurationSeconds $DurationSeconds -EvidenceRoot $runRoot -LabPath $resolvedLabPath 1> $runtimeStdout 2> $runtimeStderr
    }
    catch {
        throw "AEC runtime validation failed: $($_.Exception.Message) $(Get-Content -LiteralPath $runtimeStderr -Raw -ErrorAction SilentlyContinue)"
    }

    if ($consumer.HasExited) {
        throw "Capture client exited before the AEC interval completed with code $($consumer.ExitCode): $(Get-Content -LiteralPath $consumerStderr -Raw -ErrorAction SilentlyContinue)"
    }
    if ($playback.HasExited) {
        throw "Playback exited before the AEC interval completed with code $($playback.ExitCode): $(Get-Content -LiteralPath $playbackStderr -Raw -ErrorAction SilentlyContinue)"
    }

    if (-not $consumer.WaitForExit(15000)) {
        throw 'Capture client did not finish and finalize its private FLAC within fifteen seconds after the AEC interval.'
    }
    if ($consumer.ExitCode -ne 0) {
        throw "Capture client failed while finalizing its private FLAC with code $($consumer.ExitCode): $(Get-Content -LiteralPath $consumerStderr -Raw -ErrorAction SilentlyContinue)"
    }
    $completed = $true
}
catch {
    $failure = $_.Exception.Message
}
finally {
    Stop-OwnedProcess -Process $playback
    Stop-OwnedProcess -Process $consumer
}

$runtimeRun = Get-ChildItem -LiteralPath $runRoot -Directory -Filter '*-aec-*' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
$eventsPath = $null
if ($null -ne $runtimeRun) {
    $eventsPath = Get-ChildItem -LiteralPath $runtimeRun.FullName -Recurse -File -Filter 'engine.jsonl' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
}

$summary = [ordered]@{
    schema_version = 1
    completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
    success = $completed
    failure = $failure
    run_root = $runRoot
    runtime_run_root = if ($null -ne $runtimeRun) { $runtimeRun.FullName } else { $null }
    events_path = if ($null -ne $eventsPath) { $eventsPath.FullName } else { $null }
    recording_path = $recordingPath
    capture_client_exit_code = if ($null -ne $consumer -and $consumer.HasExited) { $consumer.ExitCode } else { $null }
    playback_exited_early = $null -ne $playback -and $playback.HasExited -and -not $completed
    capture_client_exited_early = $null -ne $consumer -and $consumer.HasExited -and -not $completed
    machine_verified_client_process_covered_interval = $completed
    machine_verified_playback_process_covered_interval = $completed
    operator_observation_still_required = $true
}
$summaryPath = Join-Path $runRoot 'orchestration-result.json'
$summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding utf8

if (-not $completed) {
    throw $failure
}
if ($null -eq $eventsPath) {
    throw "AEC completed but no engine.jsonl was found below $runRoot"
}

$revision = (& git -C $repositoryRoot rev-parse HEAD 2>$null | Select-Object -First 1)
$reportStdout = Join-Path $runRoot 'stability-report.stdout.txt'
$reportStderr = Join-Path $runRoot 'stability-report.stderr.txt'
& $resolvedLabPath stability-report --events $eventsPath.FullName --software-revision $revision 1> $reportStdout 2> $reportStderr
if ($LASTEXITCODE -ne 0) {
    throw "Stability report generation failed with exit code $LASTEXITCODE`: $(Get-Content -LiteralPath $reportStderr -Raw -ErrorAction SilentlyContinue)"
}

Write-Host "Unattended long-run capture completed. Private evidence: $runRoot"
Write-Host 'The machine verified playback/client process coverage; the authoritative functional gate still requires a truthful operator observation sidecar.'
