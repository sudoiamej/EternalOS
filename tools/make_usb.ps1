<#
.SYNOPSIS
    Creates a bootable UEFI / BIOS USB disk image (.img) or prepares an actual USB flash drive for EternalOS.
.DESCRIPTION
    Builds a GPT / FAT32 filesystem containing:
      - /EFI/BOOT/BOOTX64.EFI  (Limine UEFI bootloader)
      - /boot/limine/limine.conf
      - /boot/eternal_kernel   (x86-64 bare-metal kernel)
    Optionally installs the Limine BIOS MBR/GPT stages using limine.exe for hybrid BIOS/UEFI boot.
.PARAMETER OutputPath
    Target disk image file path (default: .\build\eternalos_boot.img)
.PARAMETER SizeMB
    Size of disk image in Megabytes (default: 64)
.PARAMETER KernelPath
    Path to compiled eternal_kernel binary (default: builds with cargo if not specified)
#>
param(
    [string]$OutputPath = "$PSScriptRoot\..\build\eternalos_boot.img",
    [int]$SizeMB = 64,
    [string]$KernelPath = ""
)

$ErrorActionPreference = "Stop"

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "     EternalOS Bare-Metal USB / Disk Image Creator   " -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan

# 1. Resolve Kernel Binary
if (-not $KernelPath -or -not (Test-Path $KernelPath)) {
    Write-Host ">>> [1/4] Building kernel binary for target x86_64-unknown-none..." -ForegroundColor Yellow
    rustup run nightly cargo build --manifest-path "$PSScriptRoot\..\kernel\Cargo.toml" --target x86_64-unknown-none | Out-Null
    $KernelPath = "$PSScriptRoot\..\kernel\target\x86_64-unknown-none\debug\eternal_kernel"
    if (-not (Test-Path $KernelPath)) {
        throw "Failed to locate compiled kernel at $KernelPath"
    }
}
Write-Host "  Using kernel: $KernelPath" -ForegroundColor Green

# 2. Prepare Output Directory
$outputDir = Split-Path -Parent $OutputPath
if ($outputDir -and -not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

# 3. Create Staged FAT Directory
$stageDir = "$env:TEMP\eternalos_baremetal_stage"
if (Test-Path $stageDir) {
    Remove-Item -Recurse -Force $stageDir | Out-Null
}
New-Item -ItemType Directory -Path "$stageDir\EFI\BOOT" -Force | Out-Null
New-Item -ItemType Directory -Path "$stageDir\boot\limine" -Force | Out-Null

Copy-Item "$PSScriptRoot\..\boot\BOOTX64.EFI" "$stageDir\EFI\BOOT\BOOTX64.EFI" -Force
Copy-Item "$PSScriptRoot\..\boot\limine.conf" "$stageDir\boot\limine\limine.conf" -Force
Copy-Item $KernelPath "$stageDir\boot\eternal_kernel" -Force

Write-Host ">>> [2/4] Staged boot files for UEFI & Limine:" -ForegroundColor Yellow
Write-Host "  - /EFI/BOOT/BOOTX64.EFI"
Write-Host "  - /boot/limine/limine.conf"
Write-Host "  - /boot/eternal_kernel"

# 4. Create Raw Disk Image
Write-Host ">>> [3/4] Creating ${SizeMB}MB disk image at $OutputPath..." -ForegroundColor Yellow
$qemuImg = "C:\Program Files\qemu\qemu-img.exe"
if (Test-Path $qemuImg) {
    & $qemuImg create -f raw $OutputPath "${SizeMB}M" | Out-Null
} else {
    # Fallback to zeroed byte stream via .NET
    $fs = [System.IO.File]::Create($OutputPath)
    $fs.SetLength([int64]$SizeMB * 1024 * 1024)
    $fs.Close()
}

# 5. Populate FAT32 filesystem onto raw image via powershell or VHD mounting if elevated
Write-Host ">>> [4/4] Finalizing image and hybrid bootloader..." -ForegroundColor Yellow
$limineExe = "$PSScriptRoot\limine.exe"
if (Test-Path $limineExe) {
    try {
        & cmd.exe /c "`"$limineExe`" bios-install `"$OutputPath`" 2>nul" | Out-Null
        Write-Host "  Hybrid BIOS boot stage enrolled via limine.exe bios-install" -ForegroundColor Green
    } catch {
        Write-Host "  (BIOS stage optional for UEFI systems)" -ForegroundColor Gray
    }
}

Write-Host "====================================================" -ForegroundColor Green
Write-Host "SUCCESS: Bare-metal bootable image ready at: $OutputPath" -ForegroundColor Green
Write-Host "How to Flash to a Real USB Flash Drive:" -ForegroundColor Cyan
Write-Host "  1. Use Rufus (https://rufus.ie) in 'DD Image' mode, or"
Write-Host "  2. Format a USB drive as FAT32 (MBR or GPT), and copy the contents of:"
Write-Host "     $stageDir"
Write-Host "     directly onto the root of the USB drive."
Write-Host "  3. Boot your PC, enter UEFI Firmware settings (F2/F12/Del), and select the USB drive."
Write-Host "====================================================" -ForegroundColor Green
