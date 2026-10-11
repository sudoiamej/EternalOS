param(
    [switch]$Headless,
    [switch]$Gdb,
    [string]$Feature
)

$ErrorActionPreference = "Stop"

$env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User") + ";C:\Program Files\qemu;$env:USERPROFILE\.cargo\bin"

Write-Host ">>> [1/4] Building EternalOS Kernel (x86_64-unknown-none)..." -ForegroundColor Cyan
Push-Location "$PSScriptRoot\kernel"
try {
    $env:RUSTFLAGS = "-C relocation-model=static"
    $buildArgs = @("build", "--target", "x86_64-unknown-none")
    if ($Feature) {
        $buildArgs += @("--features", $Feature)
    }
    cargo @buildArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo build failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$kernelBin = "$PSScriptRoot\kernel\target\x86_64-unknown-none\debug\eternal_kernel"
if (-not (Test-Path $kernelBin)) {
    throw "Kernel binary not found at $kernelBin"
}

Write-Host ">>> [2/4] Staging Boot Directory Structure..." -ForegroundColor Cyan
$stageDir = "$env:TEMP\eternalos_boot_stage"
if (Test-Path $stageDir) {
    try {
        Remove-Item -Recurse -Force $stageDir -ErrorAction Stop
    } catch {
        $stageDir = "$env:TEMP\eternalos_boot_stage_" + [System.Diagnostics.Process]::GetCurrentProcess().Id
    }
}
New-Item -ItemType Directory -Path "$stageDir\EFI\BOOT" -Force | Out-Null
New-Item -ItemType Directory -Path "$stageDir\boot\limine" -Force | Out-Null

Copy-Item "$PSScriptRoot\boot\BOOTX64.EFI" "$stageDir\EFI\BOOT\BOOTX64.EFI" -Force
Copy-Item "$PSScriptRoot\boot\limine.conf" "$stageDir\boot\limine\limine.conf" -Force
Copy-Item $kernelBin "$stageDir\boot\eternal_kernel" -Force

Write-Host ">>> [3/4] Preparing QEMU Firmware & Storage..." -ForegroundColor Cyan
$ovmf = "C:\Program Files\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) {
    throw "OVMF UEFI firmware not found at $ovmf"
}

# Ensure disposable test disk exists for block storage validation
$testDisk = "$env:TEMP\eternalos_test_disk.img"
if (-not (Test-Path $testDisk)) {
    & "C:\Program Files\qemu\qemu-img.exe" create -f raw $testDisk 10M | Out-Null
}
# Format test disk with valid EternalFS filesystem on Partition 0
& powershell -ExecutionPolicy Bypass -File "$PSScriptRoot\tools\make_eternalfs.ps1" -ImagePath $testDisk | Out-Null

$qemuArgs = @(
    "-M", "q35",
    "-m", "512M",
    "-drive", "if=pflash,unit=0,format=raw,file=$ovmf,readonly=on",
    "-drive", "file=fat:rw:$stageDir,format=raw",
    "-drive", "id=testdisk,file=$testDisk,format=raw,if=none",
    "-device", "virtio-blk-pci,drive=testdisk",
    "-serial", "stdio",
    "-no-reboot"
)

if ($Headless) {
    $qemuArgs += @("-display", "none")
}

if ($Gdb) {
    Write-Host "[GDB] Listening on localhost:1234..." -ForegroundColor Yellow
    $qemuArgs += @("-s", "-S")
}

Write-Host ">>> [4/4] Launching QEMU..." -ForegroundColor Green
& "C:\Program Files\qemu\qemu-system-x86_64.exe" @qemuArgs
