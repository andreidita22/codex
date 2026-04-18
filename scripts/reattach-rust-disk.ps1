param(
    [string]$Distro = "Ubuntu",
    [string]$VhdPath = "C:\WSL\rust-build.vhdx",
    [string]$MountPoint = "/mnt/rust-build",
    [switch]$NoPause
)

$ErrorActionPreference = "Stop"
$WslExe = "C:\Windows\System32\wsl.exe"

$ScriptPath = if ($PSCommandPath) {
    $PSCommandPath
}
elseif ($MyInvocation.PSCommandPath) {
    $MyInvocation.PSCommandPath
}
elseif ($MyInvocation.MyCommand.Path) {
    $MyInvocation.MyCommand.Path
}
else {
    $null
}

function Ensure-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    $isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

    if ($isAdmin) {
        return
    }

    if (-not $ScriptPath) {
        throw "Could not resolve script path for elevation."
    }

    Write-Host "Re-launching as Administrator..." -ForegroundColor Yellow
    $argList = @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $ScriptPath,
        "-Distro",
        $Distro,
        "-VhdPath",
        $VhdPath,
        "-MountPoint",
        $MountPoint
    )
    if ($NoPause) {
        $argList += "-NoPause"
    }

    Start-Process -FilePath "powershell.exe" -Verb RunAs -ArgumentList $argList -Wait | Out-Null
    exit
}

function Invoke-Wsl {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Args
    )

    & $WslExe @Args
    if ($LASTEXITCODE -ne 0) {
        throw "wsl.exe command failed with exit code ${LASTEXITCODE}: wsl $($Args -join ' ')"
    }
}

function Get-RustMountUuid {
    $fstab = (& $WslExe -d $Distro -u root -- cat /etc/fstab)
    if ($LASTEXITCODE -ne 0) {
        throw "failed to read /etc/fstab from distro $Distro"
    }

    $entry = $fstab | Select-String -Pattern '^\s*UUID=([^\s]+)\s+/mnt/rust-build\s+'
    if (-not $entry) {
        throw "No /mnt/rust-build entry found in /etc/fstab"
    }

    return $entry.Matches[0].Groups[1].Value
}

function Get-MountHealth {
    param(
        [Parameter(Mandatory = $true)]
        [string]$MountPoint
    )

    $status = (& $WslExe -d $Distro -- bash -lc "mountpoint -q $MountPoint && echo mounted || echo not-mounted").Trim()
    if ($status -ne "mounted") {
        return "not-mounted"
    }

    $readable = (& $WslExe -d $Distro -- bash -lc "ls $MountPoint >/dev/null 2>&1 && echo healthy || echo stale").Trim()
    if ($readable -eq "healthy") {
        return "healthy"
    }

    return "stale"
}

function Wait-ForUuid {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Uuid,
        [int]$TimeoutSeconds = 20
    )

    for ($i = 0; $i -lt $TimeoutSeconds; $i++) {
        $resolved = (& $WslExe -d $Distro -- bash -lc "blkid -U $Uuid || true").Trim()
        if ($LASTEXITCODE -eq 0 -and $resolved) {
            return $resolved
        }
        Start-Sleep -Seconds 1
    }

    throw "Timed out waiting for UUID $Uuid to appear inside WSL."
}

function Ensure-RustMount {
    param(
        [Parameter(Mandatory = $true)]
        [string]$MountPoint,
        [Parameter(Mandatory = $true)]
        [string]$Uuid,
        [int]$Retries = 10
    )

    for ($i = 0; $i -lt $Retries; $i++) {
        if ((Get-MountHealth -MountPoint $MountPoint) -eq "healthy") {
            return
        }

        Invoke-Wsl -Args @("-d", $Distro, "-u", "root", "--", "mount", "-a")
        Start-Sleep -Seconds 1
    }

    $device = (& $WslExe -d $Distro -- bash -lc "blkid -U $Uuid || true").Trim()
    if ($device) {
        Write-Host "Direct mount fallback via $device ..." -ForegroundColor Yellow
        Invoke-Wsl -Args @("-d", $Distro, "-u", "root", "--", "mount", $device, $MountPoint)
        return
    }

    throw "Rust disk did not mount at $MountPoint."
}

try {
    Ensure-Admin

    Write-Host "Rust disk reattach utility" -ForegroundColor Green
    Write-Host "Distro: $Distro"
    Write-Host "VHD:    $VhdPath"
    Write-Host ""

    if (-not (Test-Path -LiteralPath $VhdPath)) {
        throw "VHD not found: $VhdPath"
    }

    $status = Get-MountHealth -MountPoint $MountPoint
    if ($status -eq "healthy") {
        Write-Host "Mount point already active: $MountPoint" -ForegroundColor Green
    }
    else {
        if ($status -eq "stale") {
            Write-Host "Detected stale rust mount; clearing it first..." -ForegroundColor Yellow
            Invoke-Wsl -Args @("-d", $Distro, "-u", "root", "--", "umount", "-lf", $MountPoint)
        }

        $uuid = Get-RustMountUuid

        Write-Host "Attaching VHD to WSL..." -ForegroundColor Cyan
        Invoke-Wsl -Args @("--mount", "--vhd", $VhdPath, "--bare")

        Write-Host "Waiting for rust disk to appear in WSL..." -ForegroundColor Cyan
        $device = Wait-ForUuid -Uuid $uuid
        Write-Host "Rust disk visible as $device" -ForegroundColor Cyan

        Write-Host "Applying mounts from /etc/fstab..." -ForegroundColor Cyan
        Ensure-RustMount -MountPoint $MountPoint -Uuid $uuid
    }

    $verify = Get-MountHealth -MountPoint $MountPoint
    if ($verify -ne "healthy") {
        Write-Host "Mount verification failed for $MountPoint" -ForegroundColor Red
        Write-Host "Diagnostic lsblk output:" -ForegroundColor Yellow
        & $WslExe -d $Distro -- bash -lc "lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINT"
        throw "Rust disk is not mounted."
    }

    Write-Host "Rust disk mounted successfully at $MountPoint" -ForegroundColor Green
    Write-Host "Current block devices:" -ForegroundColor Cyan
    & $WslExe -d $Distro -- bash -lc "lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINT"
}
catch {
    Write-Host ""
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
finally {
    if (-not $NoPause) {
        Write-Host ""
        Read-Host "Press Enter to close"
    }
}
