param(
    [string]$ImagePath = "$env:TEMP\eternalos_test_disk.img"
)

$ErrorActionPreference = "Stop"

Write-Host ">>> Constructing EternalFS Partition Image on $ImagePath..." -ForegroundColor Cyan

# We use the standard IEEE 802.3 CRC32 polynomial 0xEDB88320 table
$poly = [uint32]3988292384 # 0xEDB88320
$crcTable = New-Object uint32[] 256
for ($i = 0; $i -lt 256; $i++) {
    $c = [uint32]$i
    for ($j = 0; $j -lt 8; $j++) {
        if (($c -band 1) -ne 0) {
            $c = [uint32](($c -shr 1) -bxor $poly)
        } else {
            $c = [uint32]($c -shr 1)
        }
    }
    $crcTable[$i] = $c
}

function Get-Crc32([byte[]]$data, [int]$offset, [int]$length) {
    $crc = [uint32]4294967295
    for ($k = 0; $k -lt $length; $k++) {
        $b = [uint32]$data[$offset + $k]
        $tableIdx = ($crc -bxor $b) -band [uint32]0xFF
        $crc = ($crc -shr 8) -bxor $crcTable[$tableIdx]
    }
    return [uint32]($crc -bxor [uint32]4294967295)
}

# Ensure disk image exists (10 MiB)
if (-not (Test-Path $ImagePath)) {
    & "C:\Program Files\qemu\qemu-img.exe" create -f raw $ImagePath 10M | Out-Null
}

$fs = [System.IO.File]::Open($ImagePath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite)

try {
    # -------------------------------------------------------------
    # 1. Setup Protective MBR & GPT if not already partitioned
    # -------------------------------------------------------------
    # Partition 0: LBA 2048 .. LBA 6143 (4096 sectors = 2 MiB, 2048 blocks)
    $partStartLba = 2048
    $partSectorCount = 4096
    $partByteOffset = [int64]$partStartLba * 512

    # Write MBR Partition Entry 0 pointing to LBA 2048
    $mbr = New-Object byte[] 512
    $fs.Seek(0, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Read($mbr, 0, 512) | Out-Null

    # Set MBR partition 1
    # Offset 446: Bootable (0x80)
    $mbr[446] = 0x80
    # Offset 446 + 4: Type 0x83 (Linux / native filesystem)
    $mbr[446 + 4] = 0x83
    # LBA Start: 2048 (0x00000800)
    [System.BitConverter]::GetBytes([uint32]$partStartLba).CopyTo($mbr, 446 + 8)
    # Sector Count: 4096 (0x00001000)
    [System.BitConverter]::GetBytes([uint32]$partSectorCount).CopyTo($mbr, 446 + 12)
    # Signature 0x55, 0xAA
    $mbr[510] = 0x55
    $mbr[511] = 0xAA

    $fs.Seek(0, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($mbr, 0, 512)

    # -------------------------------------------------------------
    # 2. Construct EternalFS Blocks (1024 bytes each)
    # -------------------------------------------------------------
    # Block 0 (Bytes 0..1023): Boot / Reserved (all zeros)
    # Block 1 (Bytes 1024..2047): Superblock
    # Block 2 (Bytes 2048..3071): Inode Table (Inodes 1..16, 64 bytes each)
    # Block 3 (Bytes 3072..4095): Root Directory Entries
    # Block 4 (Bytes 4096..5119): File Data: /hello.txt
    # Block 5 (Bytes 5120..6143): File Data: /multiblock.txt Block 0
    # Block 6 (Bytes 6144..7167): File Data: /multiblock.txt Block 1
    # Block 7 (Bytes 7168..8191): Dir Data: /docs Directory Entries
    # Block 8 (Bytes 8192..9215): File Data: /docs/kernel.txt

    # --- Superblock (Block 1) ---
    $sb = New-Object byte[] 1024
    # Magic b"ETERNAL1"
    [System.Text.Encoding]::ASCII.GetBytes("ETERNAL1").CopyTo($sb, 0)
    # Version: 1
    [System.BitConverter]::GetBytes([uint32]1).CopyTo($sb, 0x08)
    # Block Size: 1024
    [System.BitConverter]::GetBytes([uint32]1024).CopyTo($sb, 0x0C)
    # Total Blocks: 2048
    [System.BitConverter]::GetBytes([uint64]2048).CopyTo($sb, 0x10)
    # Total Inodes: 16
    [System.BitConverter]::GetBytes([uint64]16).CopyTo($sb, 0x18)
    # Inode Table Block: 2
    [System.BitConverter]::GetBytes([uint64]2).CopyTo($sb, 0x20)
    # Data Block Start: 3
    [System.BitConverter]::GetBytes([uint64]3).CopyTo($sb, 0x28)
    # Root Inode: 1
    [System.BitConverter]::GetBytes([uint64]1).CopyTo($sb, 0x30)
    # Flags: 0
    [System.BitConverter]::GetBytes([uint32]0).CopyTo($sb, 0x38)

    # Compute CRC32 for 0x00..0x3C
    $sbCrc = Get-Crc32 $sb 0 0x3C
    [System.BitConverter]::GetBytes([uint32]$sbCrc).CopyTo($sb, 0x3C)

    # --- Inode Table (Block 2) ---
    # 16 inodes * 64 bytes = 1024 bytes
    $inodes = New-Object byte[] 1024

    function Write-Inode([int]$inodeNum, [uint16]$type, [uint16]$permissions, [uint64]$size, [uint32[]]$direct, [uint32]$indirect) {
        $idx = $inodeNum - 1
        $off = $idx * 64
        # Offset 0x00..0x02: node_type
        [System.BitConverter]::GetBytes([uint16]$type).CopyTo($inodes, $off + 0x00)
        # Offset 0x02..0x04: flags
        [System.BitConverter]::GetBytes([uint16]$permissions).CopyTo($inodes, $off + 0x02)
        # Offset 0x04..0x08: reserved (0)
        [System.BitConverter]::GetBytes([uint32]0).CopyTo($inodes, $off + 0x04)
        # Offset 0x08..0x10: file_size
        [System.BitConverter]::GetBytes([uint64]$size).CopyTo($inodes, $off + 0x08)
        # Offset 0x10..0x38: Direct Blocks (10 entries = 40 bytes)
        for ($b = 0; $b -lt 10; $b++) {
            $blkNum = [uint32]0
            if ($direct -and $b -lt $direct.Length) {
                $blkNum = $direct[$b]
            }
            [System.BitConverter]::GetBytes([uint32]$blkNum).CopyTo($inodes, $off + 0x10 + ($b * 4))
        }
        # Offset 0x38..0x3C: Single Indirect
        [System.BitConverter]::GetBytes([uint32]$indirect).CopyTo($inodes, $off + 0x38)
        # Offset 0x3C..0x40: Compute CRC32 of 0x00..0x3C
        $iCrc = Get-Crc32 $inodes $off 0x3C
        [System.BitConverter]::GetBytes([uint32]$iCrc).CopyTo($inodes, $off + 0x3C)
    }

    # Inode 1: Root directory "/" (Dir size = 4 entries * 32 = 128 bytes, Block 3)
    Write-Inode 1 2 0x755 128 @([uint32]3) 0

    # Inode 2: "/hello.txt" (Size 26 bytes, Block 4)
    Write-Inode 2 1 0x644 26 @([uint32]4) 0

    # Inode 3: "/empty.txt" (Size 0 bytes, No blocks)
    Write-Inode 3 1 0x644 0 @() 0

    # Inode 4: "/multiblock.txt" (Size 1500 bytes, Block 5 and Block 6)
    Write-Inode 4 1 0x644 1500 @([uint32]5, [uint32]6) 0

    # Inode 5: "/docs" directory (Dir size = 3 entries * 32 = 96 bytes, Block 7)
    Write-Inode 5 2 0x755 96 @([uint32]7) 0

    # Inode 6: "/docs/kernel.txt" (Size 49 bytes, Block 8)
    Write-Inode 6 1 0x644 49 @([uint32]8) 0

    # --- Directory Block 3 (Root Directory) ---
    # Entries: ".", "hello.txt", "empty.txt", "multiblock.txt", "docs"
    # Total = 5 entries * 32 = 160 bytes
    $rootEntries = New-Object byte[] 1024

    function Write-Dirent([byte[]]$buf, [int]$entryIdx, [uint32]$inodeNum, [byte]$type, [string]$name) {
        $off = $entryIdx * 32
        [System.BitConverter]::GetBytes([uint32]$inodeNum).CopyTo($buf, $off + 0x00)
        $buf[$off + 0x04] = $type
        $nameBytes = [System.Text.Encoding]::ASCII.GetBytes($name)
        $len = [Math]::Min($nameBytes.Length, 26)
        $buf[$off + 0x05] = [byte]$len
        for ($i = 0; $i -lt $len; $i++) {
            $buf[$off + 0x06 + $i] = $nameBytes[$i]
        }
    }

    Write-Dirent $rootEntries 0 1 2 "."
    Write-Dirent $rootEntries 1 2 1 "hello.txt"
    Write-Dirent $rootEntries 2 3 1 "empty.txt"
    Write-Dirent $rootEntries 3 4 1 "multiblock.txt"
    Write-Dirent $rootEntries 4 5 2 "docs"

    # Update Inode 1 size to 160 bytes
    Write-Inode 1 2 0x755 160 @([uint32]3) 0

    # --- Block 4: "/hello.txt" ---
    $helloBytes = New-Object byte[] 1024
    $helloStr = "Hello from EternalFS v1.0!`n"
    [System.Text.Encoding]::ASCII.GetBytes($helloStr).CopyTo($helloBytes, 0)

    # --- Block 5 & 6: "/multiblock.txt" (1500 bytes) ---
    $multiBlock0 = New-Object byte[] 1024
    $multiBlock1 = New-Object byte[] 1024
    # Pattern: 'A' * 1024 for block 0, 'B' * 476 for block 1
    for ($i = 0; $i -lt 1024; $i++) { $multiBlock0[$i] = [byte]65 } # 'A'
    for ($i = 0; $i -lt 476; $i++) { $multiBlock1[$i] = [byte]66 }  # 'B'

    # --- Block 7: "/docs" Directory ---
    $docsEntries = New-Object byte[] 1024
    Write-Dirent $docsEntries 0 5 2 "."
    Write-Dirent $docsEntries 1 1 2 ".."
    Write-Dirent $docsEntries 2 6 1 "kernel.txt"

    # --- Block 8: "/docs/kernel.txt" ---
    $kernelDocBytes = New-Object byte[] 1024
    $kernelDocStr = "EternalOS: Rust-first hybrid x86-64 microkernel.`n"
    [System.Text.Encoding]::ASCII.GetBytes($kernelDocStr).CopyTo($kernelDocBytes, 0)

    # -------------------------------------------------------------
    # 3. Write all blocks to the image file at partition offset
    # -------------------------------------------------------------
    $fs.Seek($partByteOffset + 1024 * 0, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write((New-Object byte[] 1024), 0, 1024) # Block 0

    $fs.Seek($partByteOffset + 1024 * 1, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($sb, 0, 1024) # Block 1

    $fs.Seek($partByteOffset + 1024 * 2, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($inodes, 0, 1024) # Block 2

    $fs.Seek($partByteOffset + 1024 * 3, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($rootEntries, 0, 1024) # Block 3

    $fs.Seek($partByteOffset + 1024 * 4, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($helloBytes, 0, 1024) # Block 4

    $fs.Seek($partByteOffset + 1024 * 5, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($multiBlock0, 0, 1024) # Block 5

    $fs.Seek($partByteOffset + 1024 * 6, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($multiBlock1, 0, 1024) # Block 6

    $fs.Seek($partByteOffset + 1024 * 7, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($docsEntries, 0, 1024) # Block 7

    $fs.Seek($partByteOffset + 1024 * 8, [System.IO.SeekOrigin]::Begin) | Out-Null
    $fs.Write($kernelDocBytes, 0, 1024) # Block 8

    $fs.Flush()
    Write-Host ">>> EternalFS format complete on Partition 0 (LBA $partStartLba..$($partStartLba + $partSectorCount))!" -ForegroundColor Green
}
finally {
    $fs.Close()
}
