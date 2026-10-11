$stageDir = "$env:TEMP\eternalos_boot_stage"
$ovmf = "C:\Program Files\qemu\share\edk2-x86_64-code.fd"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$ppmPath = "$PSScriptRoot\screenshot.ppm"
$pngPath = "$PSScriptRoot\screenshot.png"

Write-Host ">>> Launching QEMU with QMP socket..." -ForegroundColor Cyan

$process = Start-Process -FilePath $qemu -ArgumentList @(
    "-M", "q35",
    "-m", "512M",
    "-drive", "if=pflash,unit=0,format=raw,file=$ovmf,readonly=on",
    "-drive", "file=fat:rw:$stageDir,format=raw",
    "-serial", "none",
    "-display", "none",
    "-no-reboot",
    "-qmp", "tcp:127.0.0.1:4444,server=on,wait=off"
) -PassThru

try {
    Start-Sleep -Seconds 6

    Write-Host ">>> Connecting to QMP..." -ForegroundColor Cyan
    $client = New-Object System.Net.Sockets.TcpClient("localhost", 4444)
    $stream = $client.GetStream()
    $reader = New-Object System.IO.StreamReader($stream)
    $writer = New-Object System.IO.StreamWriter($stream)
    $writer.AutoFlush = $true

    # Read greeting
    $line = $reader.ReadLine()
    Write-Host "QMP Greeting: $line"

    # Send qmp_capabilities
    $writer.WriteLine('{"execute": "qmp_capabilities"}')
    $line = $reader.ReadLine()
    Write-Host "QMP Cap Response: $line"

    # Send screendump command
    $escapedPpm = $ppmPath.Replace('\', '/')
    $cmd = '{"execute": "screendump", "arguments": {"filename": "' + $escapedPpm + '"}}'
    Write-Host "QMP Cmd: $cmd"
    $writer.WriteLine($cmd)
    $line = $reader.ReadLine()
    Write-Host "QMP Screendump Response: $line"

    $client.Close()

    if (Test-Path $ppmPath) {
        Write-Host "PPM screenshot captured at $ppmPath ($((Get-Item $ppmPath).Length) bytes)." -ForegroundColor Green
        # Convert PPM to PNG using System.Drawing
        Add-Type -AssemblyName System.Drawing
        $bytes = [System.IO.File]::ReadAllBytes($ppmPath)
        # Parse P6 PPM header
        # Header is: P6\nwidth height\nmaxval\n
        $idx = 0
        while ($bytes[$idx] -ne 0x0A) { $idx++ } # skip P6
        $idx++
        while ($bytes[$idx] -eq 0x23) { # skip comment lines starting with #
            while ($bytes[$idx] -ne 0x0A) { $idx++ }
            $idx++
        }
        $dimStr = ""
        while ($bytes[$idx] -ne 0x0A) { $dimStr += [char]$bytes[$idx]; $idx++ }
        $idx++
        $maxValStr = ""
        while ($bytes[$idx] -ne 0x0A) { $maxValStr += [char]$bytes[$idx]; $idx++ }
        $idx++

        $dims = $dimStr.Trim().Split(" ")
        $w = [int]$dims[0]
        $h = [int]$dims[1]
        Write-Host "Parsed PPM dimensions: $w x $h (data offset: $idx)"

        $bmp = New-Object System.Drawing.Bitmap $w, $h
        $rect = New-Object System.Drawing.Rectangle 0, 0, $w, $h
        $bmpData = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::WriteOnly, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
        $stride = $bmpData.Stride
        $scan0 = $bmpData.Scan0

        $rgbRow = New-Object byte[] ($w * 3)
        for ($y = 0; $y -lt $h; $y++) {
            $rowPtr = [IntPtr]($scan0.ToInt64() + ($y * $stride))
            # PPM is RGB, BMP 24bpp is BGR
            for ($x = 0; $x -lt $w; $x++) {
                $r = $bytes[$idx++]
                $g = $bytes[$idx++]
                $b = $bytes[$idx++]
                $rgbRow[$x * 3] = $b
                $rgbRow[($x * 3) + 1] = $g
                $rgbRow[($x * 3) + 2] = $r
            }
            [System.Runtime.InteropServices.Marshal]::Copy($rgbRow, 0, $rowPtr, $w * 3)
        }
        $bmp.UnlockBits($bmpData)
        $bmp.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        Write-Host "PNG screenshot saved successfully to $pngPath ($((Get-Item $pngPath).Length) bytes)!" -ForegroundColor Green
    } else {
        Write-Host "PPM file was not created." -ForegroundColor Red
    }
} finally {
    if (-not $process.HasExited) {
        $process.Kill()
    }
}
