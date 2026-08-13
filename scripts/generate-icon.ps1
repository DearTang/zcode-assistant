# 生成 1024x1024 应用图标源图：圆角青绿方块 + 白色折线（走势图，呼应 Logo）
Add-Type -AssemblyName System.Drawing

$out = Join-Path $PSScriptRoot "..\app-icon.png"
$resolved = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$out = Join-Path $resolved "app-icon.png"

$bmp = New-Object System.Drawing.Bitmap 1024, 1024
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# 圆角矩形路径
$r = 230
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$path.AddArc(0, 0, $r, $r, 180, 90)
$path.AddArc(1024 - $r, 0, $r, $r, 270, 90)
$path.AddArc(1024 - $r, 1024 - $r, $r, $r, 0, 90)
$path.AddArc(0, 1024 - $r, $r, $r, 90, 90)
$path.CloseFigure()

# 青绿渐变填充
$brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
  (New-Object System.Drawing.Point 0, 0),
  (New-Object System.Drawing.Point 1024, 1024),
  ([System.Drawing.Color]::FromArgb(255, 45, 212, 191)),
  ([System.Drawing.Color]::FromArgb(255, 20, 150, 170)))
$g.FillPath($brush, $path)

# 白色折线（上升走势）
$pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), 50
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
$pts = @(
  (New-Object System.Drawing.PointF 180, 660),
  (New-Object System.Drawing.PointF 400, 460),
  (New-Object System.Drawing.PointF 560, 580),
  (New-Object System.Drawing.PointF 840, 300))
$g.DrawLines($pen, $pts)

$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$brush.Dispose()
$pen.Dispose()
$bmp.Dispose()

Write-Output "icon written: $out"
