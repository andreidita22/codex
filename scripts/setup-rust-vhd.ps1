param(
    [string]$VhdPath = "C:\WSL\rust-build.vhdx",
    [UInt64]$SizeBytes = 274877906944, # 256 GiB
    [switch]$SkipShutdown
)

$ErrorActionPreference = "Stop"

function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Run this script from an elevated PowerShell (Run as Administrator)."
    }
}

Assert-Admin

$vhdDir = Split-Path -Parent $VhdPath
if (-not (Test-Path $vhdDir)) {
    New-Item -ItemType Directory -Path $vhdDir | Out-Null
}

if (-not (Test-Path $VhdPath)) {
    Write-Host "Creating VHDX at $VhdPath ..."
    New-VHD -Path $VhdPath -Dynamic -SizeBytes $SizeBytes | Out-Null
} else {
    Write-Host "VHDX already exists at $VhdPath"
}

if (-not $SkipShutdown) {
    Write-Host "Shutting down WSL before attach ..."
    wsl --shutdown
}

Write-Host "Attaching VHDX to WSL as bare disk ..."
wsl --mount --vhd $VhdPath --bare

Write-Host ""
Write-Host "Attached. Next step in WSL:"
Write-Host "  1) lsblk"
Write-Host "  2) As root: /home/rose/work/codex/fork/scripts/finalize-rust-vhd-mount.sh /dev/sdX"
Write-Host ""
Write-Host "After that, /mnt/rust-build is ready for scripts/cargo-workflow.sh"
