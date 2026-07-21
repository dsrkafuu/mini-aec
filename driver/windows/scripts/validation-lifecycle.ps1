[CmdletBinding()]
param(
    [ValidateSet('Plan', 'Inventory', 'PrepareSigning', 'Install', 'Restart', 'Uninstall')]
    [string]$Action = 'Plan',
    [switch]$ConfirmSystemChanges,
    [string]$PackageRoot,
    [string]$EvidencePath,
    [string]$PublishedInf,
    [string]$CertificateThumbprint,
    [switch]$RestoreTestSigningOff
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$driverRoot = Split-Path -Parent $scriptRoot
$repoRoot = Split-Path -Parent (Split-Path -Parent $driverRoot)
$preflightScript = Join-Path $scriptRoot 'preflight.ps1'
if (-not $PackageRoot) {
    $PackageRoot = Join-Path $driverRoot 'out\validation-x64-debug'
}
$PackageRoot = [System.IO.Path]::GetFullPath($PackageRoot)
$infPath = Join-Path $PackageRoot 'MiniAECValidation.inf'
$catalogPath = Join-Path $PackageRoot 'MiniAECValidation.cat'
$certificatePath = Join-Path $PackageRoot 'MiniAECValidation.cer'
$hardwareId = 'Root\MiniAECValidation'
$serviceName = 'MiniAECValidation'
$certificateSubject = 'CN=MiniAEC Validation Test'

function Get-Toolchain {
    $json = & $preflightScript -Json
    if ($LASTEXITCODE -ne 0) {
        throw 'Driver toolchain preflight failed.'
    }
    return $json | ConvertFrom-Json
}

function Get-DevConPath {
    param($Toolchain)

    $versions = @($Toolchain.WindowsDriverKitVersions) | Sort-Object { [version]$_ } -Descending
    foreach ($version in $versions) {
        $candidate = Join-Path $Toolchain.KitsRoot10 "Tools\$version\x64\devcon.exe"
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }
    throw 'The x64 WDK DevCon executable was not found.'
}

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'This action requires an elevated PowerShell session.'
    }
}

function Show-Plan {
    $toolchain = Get-Toolchain
    $devCon = Get-DevConPath -Toolchain $toolchain
    Write-Host 'MiniAEC validation lifecycle plan (no action is performed by Plan):'
    Write-Host "  Package: $PackageRoot"
    Write-Host "  Hardware ID: $hardwareId"
    Write-Host "  1. Create one non-exportable LocalMachine code-signing certificate: $certificateSubject"
    Write-Host '  2. Import its public certificate into LocalMachine Root and TrustedPublisher.'
    Write-Host "  3. Sign only $catalogPath with $($toolchain.SignToolPath)."
    Write-Host '  4. Run: bcdedit.exe /set testsigning on (a manual reboot is required before install).'
    Write-Host "  5. Run: `"$devCon`" install `"$infPath`" `"$hardwareId`""
    Write-Host "  6. Run: `"$devCon`" restart `"$hardwareId`""
    Write-Host "  7. Run: `"$devCon`" remove `"$hardwareId`""
    Write-Host '  8. Run: pnputil.exe /delete-driver <recorded-oem-inf> /uninstall /force'
    Write-Host '  9. Remove only the recorded certificate thumbprint from LocalMachine My, Root and TrustedPublisher.'
    Write-Host ' 10. If the saved pre-install state had test signing off, run: bcdedit.exe /set testsigning off (a manual reboot is required).'
    Write-Host 'Mutating actions refuse to run unless -ConfirmSystemChanges is supplied. Install and Restart remain separate actions from signing preparation.'
}

function Get-MiniAecDevices {
    $miniAecDevices = @()
    foreach ($device in @(Get-PnpDevice -Class Media)) {
        $hardwareIdProperty = Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction SilentlyContinue
        $hardwareIds = @($hardwareIdProperty.Data)
        if ($hardwareId -notin $hardwareIds) {
            continue
        }

        $serviceProperty = Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName 'DEVPKEY_Device_Service' -ErrorAction SilentlyContinue
        $driverInfProperty = Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName 'DEVPKEY_Device_DriverInfPath' -ErrorAction SilentlyContinue
        $miniAecDevices += [pscustomobject]@{
            Status = $device.Status
            Class = $device.Class
            FriendlyName = $device.FriendlyName
            InstanceId = $device.InstanceId
            HardwareIds = $hardwareIds
            Service = $serviceProperty.Data
            DriverInfPath = $driverInfProperty.Data
        }
    }
    return $miniAecDevices
}

function Save-Inventory {
    if (-not $EvidencePath) {
        $timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
        $EvidencePath = Join-Path $driverRoot "out\validation\inventory-$timestamp.json"
    }
    $EvidencePath = [System.IO.Path]::GetFullPath($EvidencePath)
    $evidenceDirectory = Split-Path -Parent $EvidencePath
    New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null

    $audioTool = Join-Path $repoRoot 'target\debug\mini-aec-lab.exe'
    if (-not (Test-Path -LiteralPath $audioTool -PathType Leaf)) {
        throw 'Build the read-only endpoint enumerator first with .tools\cargo-webrtc.cmd build -p mini-aec-lab.'
    }
    $audioJson = & $audioTool devices --json
    if ($LASTEXITCODE -ne 0) {
        throw 'mini-aec-lab failed to enumerate WASAPI endpoints.'
    }

    try {
        $secureBoot = Confirm-SecureBootUEFI
    } catch {
        $secureBoot = "unavailable: $($_.Exception.Message)"
    }

    $inventory = [ordered]@{
        RecordedAt = (Get-Date).ToString('o')
        ComputerName = $env:COMPUTERNAME
        Windows = (Get-Toolchain).Windows
        SecureBoot = $secureBoot
        BootConfiguration = @(bcdedit.exe /enum)
        WasapiEndpoints = $audioJson | ConvertFrom-Json
        AudioEndpointPnp = @(Get-PnpDevice -Class AudioEndpoint | Select-Object Status, Class, FriendlyName, InstanceId)
        MediaPnp = @(Get-PnpDevice -Class Media | Select-Object Status, Class, FriendlyName, InstanceId)
        MiniAecDevices = @(Get-MiniAecDevices)
        DriverPackages = @(pnputil.exe /enum-drivers /class Media)
        MiniAecCertificates = @(
            Get-ChildItem Cert:\LocalMachine\My, Cert:\LocalMachine\Root, Cert:\LocalMachine\TrustedPublisher |
                Where-Object { $_.Subject -eq $certificateSubject } |
                Select-Object Subject, Thumbprint, NotBefore, NotAfter, PSParentPath
        )
    }
    $inventory | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $EvidencePath -Encoding utf8
    Write-Host "Read-only inventory saved to $EvidencePath"
}

Show-Plan
if ($Action -eq 'Plan') {
    return
}
if ($Action -eq 'Inventory') {
    Save-Inventory
    return
}
if (-not $ConfirmSystemChanges) {
    throw "Action '$Action' changes Windows state. Review the plan and rerun with -ConfirmSystemChanges only after explicit approval."
}

Assert-Administrator
$toolchain = Get-Toolchain
$devCon = Get-DevConPath -Toolchain $toolchain

switch ($Action) {
    'PrepareSigning' {
        foreach ($artifact in @($infPath, $catalogPath)) {
            if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
                throw "Validation package artifact is missing: $artifact"
            }
        }
        $existing = @(Get-ChildItem Cert:\LocalMachine\My | Where-Object { $_.Subject -eq $certificateSubject })
        if ($existing.Count -ne 0) {
            throw "A certificate with subject '$certificateSubject' already exists. Remove or explicitly account for it before creating another."
        }
        $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject $certificateSubject -CertStoreLocation Cert:\LocalMachine\My -KeyExportPolicy NonExportable -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -NotAfter (Get-Date).AddMonths(3)
        Export-Certificate -Cert $certificate -FilePath $certificatePath -Type CERT | Out-Null
        Import-Certificate -FilePath $certificatePath -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
        Import-Certificate -FilePath $certificatePath -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
        & $toolchain.SignToolPath sign /v /fd SHA256 /s My /sm /sha1 $certificate.Thumbprint $catalogPath
        if ($LASTEXITCODE -ne 0) {
            throw "SignTool failed with exit code $LASTEXITCODE."
        }
        bcdedit.exe /set testsigning on
        if ($LASTEXITCODE -ne 0) {
            throw "BCDEdit failed with exit code $LASTEXITCODE. Secure Boot may need separate user action."
        }
        Write-Host "Certificate thumbprint: $($certificate.Thumbprint)"
        Write-Host 'Signing preparation completed. Reboot Windows manually, then rerun Inventory and verify TESTSIGNING before Install.'
    }
    'Install' {
        & $toolchain.SignToolPath verify /v /pa $catalogPath
        if ($LASTEXITCODE -ne 0) {
            throw 'The validation catalog does not pass test-signature verification.'
        }
        & $devCon install $infPath $hardwareId
        if ($LASTEXITCODE -ne 0) {
            throw "DevCon install failed with exit code $LASTEXITCODE."
        }
        Write-Host 'Installation completed. Record the published oem*.inf name from pnputil.exe /enum-drivers /class Media before uninstall.'
    }
    'Restart' {
        & $devCon restart $hardwareId
        $restartExitCode = $LASTEXITCODE
        if ($restartExitCode -eq 1) {
            throw 'DevCon reports that restarting MiniAEC requires a Windows reboot. No reboot was performed. Save a fresh Inventory and obtain explicit approval before rebooting Windows.'
        }
        if ($restartExitCode -ne 0) {
            throw "DevCon restart failed with exit code $restartExitCode."
        }
    }
    'Uninstall' {
        if (-not $PublishedInf -or $PublishedInf -notmatch '^oem\d+\.inf$') {
            throw 'Uninstall requires the exact recorded -PublishedInf oem<number>.inf value.'
        }
        if (-not $CertificateThumbprint -or $CertificateThumbprint -notmatch '^[0-9A-Fa-f]{40}$') {
            throw 'Uninstall requires the exact recorded 40-character -CertificateThumbprint value.'
        }
        & $devCon remove $hardwareId
        if ($LASTEXITCODE -ne 0) {
            throw "DevCon remove failed with exit code $LASTEXITCODE."
        }
        pnputil.exe /delete-driver $PublishedInf /uninstall /force
        if ($LASTEXITCODE -ne 0) {
            throw "PnPUtil package deletion failed with exit code $LASTEXITCODE."
        }
        foreach ($store in @('My', 'Root', 'TrustedPublisher')) {
            $certificate = Get-Item -LiteralPath "Cert:\LocalMachine\$store\$CertificateThumbprint" -ErrorAction SilentlyContinue
            if ($certificate) {
                Remove-Item -LiteralPath $certificate.PSPath -Force
            }
        }
        if ($RestoreTestSigningOff) {
            bcdedit.exe /set testsigning off
            if ($LASTEXITCODE -ne 0) {
                throw "BCDEdit rollback failed with exit code $LASTEXITCODE."
            }
            Write-Host 'Test signing was set off. Reboot Windows manually to complete boot-state rollback.'
        }
        $serviceRegistryPath = "Registry::HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\$serviceName"
        $remainingService = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
        if ($remainingService -or (Test-Path -LiteralPath $serviceRegistryPath)) {
            throw 'The MiniAEC device, package and targeted certificate were removed, but the driver service is pending deletion. No reboot was performed. Save a fresh Inventory and obtain explicit approval before rebooting Windows.'
        }
    }
}
