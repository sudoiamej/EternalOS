Add-Type -AssemblyName System.Drawing
$imgPath = Resolve-Path "assets\logo.png"
$bmp = [System.Drawing.Bitmap]::FromFile($imgPath)
$w = 64
$h = 58
$thumb = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($thumb)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.DrawImage($bmp, 0, 0, $w, $h)
$g.Dispose()
$bmp.Dispose()

$bytes = [System.Collections.Generic.List[byte]]::new()
for ($y = 0; $y -lt $h; $y++) {
    for ($bx = 0; $bx -lt ($w / 8); $bx++) {
        [byte]$b = 0
        for ($bit = 0; $bit -lt 8; $bit++) {
            $x = ($bx * 8) + $bit
            $p = $thumb.GetPixel($x, $y)
            if ($p.A -gt 80 -and ($p.R + $p.G + $p.B) -gt 120) {
                $b = $b -bor (1 -shl (7 - $bit))
            }
        }
        $bytes.Add($b)
    }
}
$thumb.Dispose()

$sb = [System.Text.StringBuilder]::new()
[void]$sb.AppendLine("//! Auto-generated 1-bit monochrome logo asset downsampled from assets/logo.png (64x58)")
[void]$sb.AppendLine("pub const LOGO_WIDTH: usize = 64;")
[void]$sb.AppendLine("pub const LOGO_HEIGHT: usize = 58;")
[void]$sb.AppendLine("pub static LOGO_BITMAP: [u8; 464] = [")

for ($i = 0; $i -lt $bytes.Count; $i++) {
    if ($i % 12 -eq 0) {
        if ($i -gt 0) { [void]$sb.AppendLine() }
        [void]$sb.Append("    ")
    }
    [void]$sb.Append(('0x{0:x2}, ' -f $bytes[$i]))
}
[void]$sb.AppendLine()
[void]$sb.AppendLine("];")

[System.IO.File]::WriteAllText("kernel\src\logo_asset.rs", $sb.ToString())
Write-Host "Generated kernel\src\logo_asset.rs successfully ($($bytes.Count) bytes)."
