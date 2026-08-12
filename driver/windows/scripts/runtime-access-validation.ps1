[CmdletBinding()]
param(
    [ValidateSet('Plan', 'Identity', 'Transport', 'Contention', 'Bypass', 'Aec')]
    [string]$Action = 'Plan',
    [string]$MicrophoneId,
    [string]$RenderId,
    [ValidateRange(1, 86400)]
    [int]$DurationSeconds = 10,
    [ValidateRange(2, 60)]
    [int]$HoldSeconds = 5,
    [string]$EvidenceRoot = 'artifacts\normal-user-access',
    [string]$SenderPath = 'target\release\mini-aec-sender.exe',
    [string]$LabPath = 'target\release\mini-aec-lab.exe'
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $driverRoot)

function Resolve-RepositoryPath {
    param([Parameter(Mandatory)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $Path))
}

$artifactsRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot 'artifacts')).TrimEnd('\')
$resolvedEvidenceRoot = (Resolve-RepositoryPath -Path $EvidenceRoot).TrimEnd('\')
if ($resolvedEvidenceRoot -ne $artifactsRoot -and -not $resolvedEvidenceRoot.StartsWith($artifactsRoot + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Runtime access evidence must remain below the ignored artifacts root: $artifactsRoot"
}

$resolvedSenderPath = Resolve-RepositoryPath -Path $SenderPath
$resolvedLabPath = Resolve-RepositoryPath -Path $LabPath

$plan = [ordered]@{
    schema_version = 1
    action = $Action.ToLowerInvariant()
    repository_root = $repositoryRoot
    evidence_root = $resolvedEvidenceRoot
    sender_path = $resolvedSenderPath
    lab_path = $resolvedLabPath
    system_changes = @()
    runtime_only = $true
    requires_non_elevated_interactive_token = $Action -ne 'Plan'
    microphone_id_supplied = -not [string]::IsNullOrWhiteSpace($MicrophoneId)
    render_id_supplied = -not [string]::IsNullOrWhiteSpace($RenderId)
}

if ($Action -eq 'Plan') {
    $plan | ConvertTo-Json -Depth 4
    return
}

if (-not ('MiniAec.Validation.TokenInspector' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace MiniAec.Validation
{
    public static class TokenInspector
    {
        [DllImport("advapi32.dll", SetLastError = true)]
        private static extern bool GetTokenInformation(
            IntPtr tokenHandle,
            int tokenInformationClass,
            out int tokenInformation,
            int tokenInformationLength,
            out int returnLength);

        public static bool IsElevated(IntPtr tokenHandle)
        {
            int elevation;
            int returned;
            if (!GetTokenInformation(tokenHandle, 20, out elevation, sizeof(int), out returned))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            return elevation != 0;
        }
    }

    public static class DeviceSecurityInspector
    {
        private const uint ReadControl = 0x00020000;
        private const uint ShareReadWrite = 0x00000003;
        private const uint OpenExisting = 3;
        private const uint FileAttributeNormal = 0x00000080;
        private const int DaclSecurityInformation = 0x00000004;

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern SafeFileHandle CreateFile(
            string fileName,
            uint desiredAccess,
            uint shareMode,
            IntPtr securityAttributes,
            uint creationDisposition,
            uint flagsAndAttributes,
            IntPtr templateFile);

        [DllImport("advapi32.dll", SetLastError = true)]
        private static extern bool GetKernelObjectSecurity(
            SafeFileHandle handle,
            int securityInformation,
            byte[] securityDescriptor,
            uint length,
            out uint lengthNeeded);

        [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool ConvertSecurityDescriptorToStringSecurityDescriptor(
            byte[] securityDescriptor,
            uint revision,
            int securityInformation,
            out IntPtr stringSecurityDescriptor,
            out uint stringLength);

        [DllImport("kernel32.dll")]
        private static extern IntPtr LocalFree(IntPtr memory);

        public static string ReadDaclSddl(string path)
        {
            using (SafeFileHandle handle = CreateFile(path, ReadControl, ShareReadWrite, IntPtr.Zero, OpenExisting, FileAttributeNormal, IntPtr.Zero))
            {
                if (handle.IsInvalid)
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                }

                uint required;
                GetKernelObjectSecurity(handle, DaclSecurityInformation, null, 0, out required);
                int sizingError = Marshal.GetLastWin32Error();
                if (required == 0 || sizingError != 122)
                {
                    throw new Win32Exception(sizingError);
                }

                byte[] descriptor = new byte[required];
                if (!GetKernelObjectSecurity(handle, DaclSecurityInformation, descriptor, required, out required))
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                }

                IntPtr text;
                uint textLength;
                if (!ConvertSecurityDescriptorToStringSecurityDescriptor(descriptor, 1, DaclSecurityInformation, out text, out textLength))
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                }
                try
                {
                    return Marshal.PtrToStringUni(text);
                }
                finally
                {
                    LocalFree(text);
                }
            }
        }
    }
}
'@
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$groupSids = @($identity.Groups | ForEach-Object { $_.Value })
$process = Get-Process -Id $PID
$tokenContext = [ordered]@{
    schema_version = 1
    captured_utc = [DateTimeOffset]::UtcNow.ToString('O')
    user_name = $identity.Name
    user_sid = $identity.User.Value
    authentication_type = $identity.AuthenticationType
    process_id = $PID
    process_session_id = $process.SessionId
    environment_user_interactive = [Environment]::UserInteractive
    token_has_interactive_sid = $groupSids -contains 'S-1-5-4'
    token_is_elevated = [MiniAec.Validation.TokenInspector]::IsElevated($identity.Token)
}

if ($tokenContext.token_is_elevated) {
    throw 'Normal-user runtime validation refuses an elevated token. Start it from an ordinary interactive terminal without Run as administrator.'
}
if (-not $tokenContext.environment_user_interactive -or -not $tokenContext.token_has_interactive_sid) {
    throw 'Normal-user runtime validation requires a local interactive Windows token.'
}

$runName = '{0}-{1}-{2}' -f [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'), $Action.ToLowerInvariant(), $PID
$runRoot = Join-Path $resolvedEvidenceRoot $runName
New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
$tokenContext | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runRoot 'identity.json') -Encoding utf8
$plan | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runRoot 'plan.json') -Encoding utf8

function Assert-Executable {
    param([Parameter(Mandatory)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required prebuilt validation executable is missing: $Path"
    }
}

function Invoke-RecordedProcess {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(Mandatory)][string[]]$ArgumentList
    )

    $stdoutPath = Join-Path $runRoot "$Name.stdout.jsonl"
    $stderrPath = Join-Path $runRoot "$Name.stderr.txt"
    $child = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -WorkingDirectory $repositoryRoot -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    return [ordered]@{
        name = $Name
        exit_code = $child.ExitCode
        stdout_path = $stdoutPath
        stderr_path = $stderrPath
        stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { '' }
    }
}

function Write-Result {
    param([Parameter(Mandatory)]$Result)

    $Result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $runRoot 'result.json') -Encoding utf8
}

function Assert-Success {
    param([Parameter(Mandatory)]$Result)

    if ($Result.exit_code -ne 0) {
        throw "Validation process '$($Result.name)' failed with exit code $($Result.exit_code): $($Result.stderr)"
    }
}

function Get-VerifiedInstalledSddl {
    $sddl = [MiniAec.Validation.DeviceSecurityInspector]::ReadDaclSddl('\\.\MiniAECTransport')
    $descriptor = [System.Security.AccessControl.RawSecurityDescriptor]::new($sddl)
    $protectedFlag = [System.Security.AccessControl.ControlFlags]::DiscretionaryAclProtected
    if (($descriptor.ControlFlags -band $protectedFlag) -eq 0) {
        throw "Installed MiniAEC transport DACL is not protected: $sddl"
    }

    $expectedAccess = @{
        'S-1-5-18'     = 0x001F01FF # LocalSystem, generic all mapped for a file object.
        'S-1-5-32-544' = 0x001F01FF # Builtin Administrators, generic all mapped for a file object.
        'S-1-5-4'      = 0x0012019F # Interactive Users, generic read plus generic write mapped for a file object.
    }
    $actualAccess = @{}
    $dacl = $descriptor.DiscretionaryAcl
    for ($index = 0; $index -lt $dacl.Count; $index++) {
        $ace = $dacl[$index]
        if ($ace -isnot [System.Security.AccessControl.CommonAce] -or
            $ace.AceQualifier -ne [System.Security.AccessControl.AceQualifier]::AccessAllowed -or
            $ace.AceFlags -ne [System.Security.AccessControl.AceFlags]::None -or
            $ace.IsCallback) {
            throw "Installed MiniAEC transport DACL contains an unexpected ACE: $sddl"
        }

        $sid = $ace.SecurityIdentifier.Value
        if (-not $expectedAccess.ContainsKey($sid) -or $actualAccess.ContainsKey($sid)) {
            throw "Installed MiniAEC transport DACL contains an unexpected or duplicate principal $sid`: $sddl"
        }
        $actualAccess[$sid] = $ace.AccessMask
    }

    if ($actualAccess.Count -ne $expectedAccess.Count) {
        throw "Installed MiniAEC transport DACL does not contain exactly the required principals: $sddl"
    }
    foreach ($sid in $expectedAccess.Keys) {
        if (-not $actualAccess.ContainsKey($sid) -or $actualAccess[$sid] -ne $expectedAccess[$sid]) {
            throw "Installed MiniAEC transport DACL has incorrect access for $sid`: $sddl"
        }
    }
    return $sddl
}

switch ($Action) {
    'Identity' {
        Write-Result -Result ([ordered]@{ action = 'identity'; success = $true; identity = $tokenContext })
    }
    'Transport' {
        Assert-Executable -Path $resolvedSenderPath
        $installedSddl = Get-VerifiedInstalledSddl
        $result = Invoke-RecordedProcess -Name 'transport' -FilePath $resolvedSenderPath -ArgumentList @('--transport', 'driver', '--access-probe')
        Write-Result -Result ([ordered]@{ action = 'transport'; success = $result.exit_code -eq 0; installed_dacl_sddl = $installedSddl; process = $result })
        Assert-Success -Result $result
    }
    'Contention' {
        Assert-Executable -Path $resolvedSenderPath
        $installedSddl = Get-VerifiedInstalledSddl
        $ownerStdout = Join-Path $runRoot 'owner.stdout.jsonl'
        $ownerStderr = Join-Path $runRoot 'owner.stderr.txt'
        $owner = Start-Process -FilePath $resolvedSenderPath -ArgumentList @('--transport', 'driver', '--access-probe', '--hold-seconds', $HoldSeconds) -WorkingDirectory $repositoryRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $ownerStdout -RedirectStandardError $ownerStderr
        try {
            $deadline = [DateTime]::UtcNow.AddSeconds(10)
            do {
                if ($owner.HasExited) {
                    throw "Owner access probe exited before opening its session: $(Get-Content -LiteralPath $ownerStderr -Raw -ErrorAction SilentlyContinue)"
                }
                $opened = (Test-Path -LiteralPath $ownerStdout -PathType Leaf) -and (Select-String -LiteralPath $ownerStdout -SimpleMatch '"event":"session_opened"' -Quiet)
                if (-not $opened) {
                    Start-Sleep -Milliseconds 100
                }
            } while (-not $opened -and [DateTime]::UtcNow -lt $deadline)
            if (-not $opened) {
                throw 'Owner access probe did not report an open session within ten seconds.'
            }

            $contender = Invoke-RecordedProcess -Name 'contender' -FilePath $resolvedSenderPath -ArgumentList @('--transport', 'driver', '--access-probe')
            if ($contender.exit_code -eq 0 -or $contender.stderr -notmatch 'Busy:') {
                throw "Second sender did not report explicit busy: $($contender.stderr)"
            }
            if (-not $owner.WaitForExit(($HoldSeconds + 10) * 1000)) {
                throw 'Owner access probe did not close within the bounded contention window.'
            }
            if ($owner.ExitCode -ne 0) {
                throw "Owner access probe failed: $(Get-Content -LiteralPath $ownerStderr -Raw -ErrorAction SilentlyContinue)"
            }

            $reconnect = Invoke-RecordedProcess -Name 'reconnect' -FilePath $resolvedSenderPath -ArgumentList @('--transport', 'driver', '--access-probe')
            Assert-Success -Result $reconnect
            Write-Result -Result ([ordered]@{ action = 'contention'; success = $true; installed_dacl_sddl = $installedSddl; owner_exit_code = $owner.ExitCode; contender = $contender; reconnect = $reconnect })
        }
        finally {
            if (-not $owner.HasExited) {
                Stop-Process -Id $owner.Id -Force
                $owner.WaitForExit()
            }
        }
    }
    'Bypass' {
        Assert-Executable -Path $resolvedLabPath
        if ([string]::IsNullOrWhiteSpace($MicrophoneId)) {
            throw 'Bypass validation requires -MicrophoneId with an exact physical capture endpoint ID.'
        }
        $engineEvidence = Join-Path $runRoot 'engine-bypass'
        $result = Invoke-RecordedProcess -Name 'bypass' -FilePath $resolvedLabPath -ArgumentList @('bypass', '--microphone-id', $MicrophoneId, '--duration', $DurationSeconds, '--output', $engineEvidence)
        Write-Result -Result ([ordered]@{ action = 'bypass'; success = $result.exit_code -eq 0; process = $result })
        Assert-Success -Result $result
    }
    'Aec' {
        Assert-Executable -Path $resolvedLabPath
        if ([string]::IsNullOrWhiteSpace($MicrophoneId) -or [string]::IsNullOrWhiteSpace($RenderId)) {
            throw 'AEC validation requires -MicrophoneId and -RenderId with exact physical endpoint IDs.'
        }
        $engineEvidence = Join-Path $runRoot 'engine-aec'
        $result = Invoke-RecordedProcess -Name 'aec' -FilePath $resolvedLabPath -ArgumentList @('realtime-aec', '--microphone-id', $MicrophoneId, '--render-id', $RenderId, '--duration', $DurationSeconds, '--output', $engineEvidence)
        Write-Result -Result ([ordered]@{ action = 'aec'; success = $result.exit_code -eq 0; process = $result })
        Assert-Success -Result $result
    }
}

Write-Host "Normal-user runtime validation action '$Action' completed. Metadata-only evidence: $runRoot"
