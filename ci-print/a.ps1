$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$files = @('C:\pc\sheet-1.png')
$sizes = @(@(210.000, 297.000))
$doc = New-Object System.Drawing.Printing.PrintDocument
$doc.PrinterSettings.PrinterName = 'Check a'
if (-not $doc.PrinterSettings.IsValid) { [Console]::Error.WriteLine('Printer not found'); exit 2 }
$doc.DocumentName = 'Check'
$doc.PrinterSettings.Copies = 1
$doc.PrinterSettings.Collate = $false
$doc.OriginAtMargins = $false
$doc.PrintController = New-Object System.Drawing.Printing.StandardPrintController
$state = @{ i = 0 }
$doc.add_QueryPageSettings({
    param($sender, $e)
    try {
    $w = $sizes[$state.i][0]; $h = $sizes[$state.i][1]
    foreach ($p in $doc.PrinterSettings.PaperSizes) {
        $pw = $p.Width * 0.254; $ph = $p.Height * 0.254
        if (([Math]::Abs($pw - [Math]::Min($w, $h)) -lt 2) -and ([Math]::Abs($ph - [Math]::Max($w, $h)) -lt 2)) {
            $e.PageSettings.PaperSize = $p
            break
        }
    }
    $e.PageSettings.Landscape = ($w -gt $h)
    } catch { [Console]::Error.WriteLine('QUERY: ' + ($_ | Out-String) + ' INNER: ' + $_.Exception.InnerException); throw }
})
$doc.add_PrintPage({
    param($sender, $e)
    try {
    $w = $sizes[$state.i][0]; $h = $sizes[$state.i][1]
    $img = [System.Drawing.Image]::FromFile($files[$state.i])
    $g = $e.Graphics
    $g.PageUnit = [System.Drawing.GraphicsUnit]::Millimeter
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $mx = $e.PageSettings.HardMarginX * 0.254; $my = $e.PageSettings.HardMarginY * 0.254
    $g.DrawImage($img, [single](-$mx), [single](-$my), [single]$w, [single]$h)
    $img.Dispose()
    $state.i++
    $e.HasMorePages = $state.i -lt $files.Count
    } catch { [Console]::Error.WriteLine('PAGE: ' + ($_ | Out-String) + ' INNER: ' + $_.Exception.InnerException); throw }
})
try { $doc.Print() } catch { [Console]::Error.WriteLine('PRINT: ' + $_.Exception.ToString()); exit 1 }
[Console]::Out.WriteLine('sent ' + $state.i + ' sheet(s)')
