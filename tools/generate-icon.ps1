[CmdletBinding()]
param(
  [string]$SvgPath = "assets/speakerphone.svg",
  [string]$OutputPath = "src-tauri/icons/icon.ico",
  [string]$TrayOutputPath = "src-tauri/icons/tray-icon.png"
)

$ErrorActionPreference = "Stop"

function Resolve-RepositoryPath {
  param([Parameter(Mandatory)][string]$Path)

  $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
  if ([System.IO.Path]::IsPathRooted($Path)) {
    return [System.IO.Path]::GetFullPath($Path)
  }

  return [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $Path))
}

function New-IconPng {
  param(
    [Parameter(Mandatory)][System.Xml.XmlNodeList]$PathNodes,
    [Parameter(Mandatory)][int]$Size,
    [Parameter(Mandatory)][double]$ViewBoxWidth,
    [Parameter(Mandatory)][double]$ViewBoxHeight,
    [Parameter(Mandatory)][System.Windows.Media.Pen]$Pen,
    [switch]$CropToVisibleBounds,
    [double]$CropPadding = 2.5
  )

  $geometries = [System.Collections.Generic.List[System.Windows.Media.Geometry]]::new()
  $visibleBounds = $null
  foreach ($pathNode in $PathNodes) {
    $pathData = $pathNode.GetAttribute("d")
    if ([string]::IsNullOrWhiteSpace($pathData)) {
      continue
    }

    $pathStroke = $pathNode.GetAttribute("stroke")
    if ($pathStroke -eq "none") {
      continue
    }

    $geometry = [System.Windows.Media.Geometry]::Parse($pathData)
    $geometries.Add($geometry)
    if ($null -eq $visibleBounds) {
      $visibleBounds = $geometry.Bounds
    } else {
      $visibleBounds = [System.Windows.Rect]::Union($visibleBounds, $geometry.Bounds)
    }
  }

  if ($geometries.Count -eq 0 -or $null -eq $visibleBounds -or $visibleBounds.IsEmpty) {
    throw "The icon source does not contain drawable geometry."
  }

  $visual = [System.Windows.Media.DrawingVisual]::new()
  $drawingContext = $visual.RenderOpen()
  if ($CropToVisibleBounds) {
    $drawableWidth = [math]::Max(1.0, $Size - (2.0 * $CropPadding))
    $drawableHeight = [math]::Max(1.0, $Size - (2.0 * $CropPadding))
    $scale = [math]::Min(
      $drawableWidth / $visibleBounds.Width,
      $drawableHeight / $visibleBounds.Height
    )
    $offsetX = (($Size - ($visibleBounds.Width * $scale)) / 2.0) - ($visibleBounds.X * $scale)
    $offsetY = (($Size - ($visibleBounds.Height * $scale)) / 2.0) - ($visibleBounds.Y * $scale)
    $matrix = [System.Windows.Media.Matrix]::new(
      $scale,
      0.0,
      0.0,
      $scale,
      $offsetX,
      $offsetY
    )
    $transform = [System.Windows.Media.MatrixTransform]::new($matrix)
    $transform.Freeze()
    $drawingContext.PushTransform($transform)
  } else {
    $drawingContext.PushTransform(
      [System.Windows.Media.ScaleTransform]::new(
        $Size / $ViewBoxWidth,
        $Size / $ViewBoxHeight
      )
    )
  }

  foreach ($geometry in $geometries) {
    $drawingContext.DrawGeometry($null, $Pen, $geometry)
  }

  $drawingContext.Pop()
  $drawingContext.Close()

  $bitmap = [System.Windows.Media.Imaging.RenderTargetBitmap]::new(
    $Size,
    $Size,
    96,
    96,
    [System.Windows.Media.PixelFormats]::Pbgra32
  )
  $bitmap.Render($visual)

  $memoryStream = [System.IO.MemoryStream]::new()
  try {
    $encoder = [System.Windows.Media.Imaging.PngBitmapEncoder]::new()
    $encoder.Frames.Add(
      [System.Windows.Media.Imaging.BitmapFrame]::Create($bitmap)
    )
    $encoder.Save($memoryStream)
    return ,$memoryStream.ToArray()
  } finally {
    $memoryStream.Dispose()
  }
}

function Write-IcoFile {
  param(
    [Parameter(Mandatory)][object[]]$Images,
    [Parameter(Mandatory)][string]$Path
  )

  $memoryStream = [System.IO.MemoryStream]::new()
  $writer = [System.IO.BinaryWriter]::new($memoryStream)
  try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$Images.Count)

    $imageOffset = 6 + (16 * $Images.Count)
    foreach ($image in $Images) {
      $dimension = if ($image.Size -ge 256) { 0 } else { [byte]$image.Size }
      $writer.Write([byte]$dimension)
      $writer.Write([byte]$dimension)
      $writer.Write([byte]0)
      $writer.Write([byte]0)
      $writer.Write([uint16]1)
      $writer.Write([uint16]32)
      $writer.Write([uint32]$image.Bytes.Length)
      $writer.Write([uint32]$imageOffset)
      $imageOffset += $image.Bytes.Length
    }

    foreach ($image in $Images) {
      $writer.Write($image.Bytes)
    }

    $writer.Flush()
    [System.IO.File]::WriteAllBytes($Path, $memoryStream.ToArray())
  } finally {
    $writer.Dispose()
    $memoryStream.Dispose()
  }
}

Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase

$resolvedSvgPath = Resolve-RepositoryPath $SvgPath
$resolvedOutputPath = Resolve-RepositoryPath $OutputPath
$svgText = [System.IO.File]::ReadAllText($resolvedSvgPath)
if (-not $svgText.Contains("currentColor")) {
  throw "The icon source must use the supplied speakerphone SVG with currentColor."
}

$xml = [System.Xml.XmlDocument]::new()
$xml.XmlResolver = $null
$xml.LoadXml($svgText)
$svgRoot = $xml.DocumentElement
$viewBox = $svgRoot.GetAttribute("viewBox") -split "\s+" |
  ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) }
if ($viewBox.Count -ne 4 -or $viewBox[0] -ne 0 -or $viewBox[1] -ne 0) {
  throw "The icon source must use a viewBox beginning at 0 0."
}

$pathNodes = $xml.SelectNodes("//*[local-name()='path']")
if ($pathNodes.Count -lt 3) {
  throw "The icon source does not contain the expected speakerphone line work."
}

$strokeWidthText = $svgRoot.GetAttribute("stroke-width")
$strokeWidth = [double]::Parse(
  $strokeWidthText,
  [Globalization.CultureInfo]::InvariantCulture
)
$iconColor = [System.Windows.Media.Color]::FromRgb(0, 0, 0)
$renderStrokeWidth = [math]::Max(0.5, $strokeWidth * 0.75)
$brush = [System.Windows.Media.SolidColorBrush]::new($iconColor)
$brush.Freeze()
$pen = [System.Windows.Media.Pen]::new($brush, $renderStrokeWidth)
$pen.LineJoin = [System.Windows.Media.PenLineJoin]::Round
$pen.StartLineCap = [System.Windows.Media.PenLineCap]::Round
$pen.EndLineCap = [System.Windows.Media.PenLineCap]::Round
$pen.Freeze()

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$images = foreach ($size in $sizes) {
  [pscustomobject]@{
    Size = $size
    Bytes = New-IconPng `
      -PathNodes $pathNodes `
      -Size $size `
      -ViewBoxWidth $viewBox[2] `
      -ViewBoxHeight $viewBox[3] `
      -Pen $pen
  }
}

$outputDirectory = Split-Path -Parent $resolvedOutputPath
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
Write-IcoFile -Images $images -Path $resolvedOutputPath

$resolvedTrayOutputPath = Resolve-RepositoryPath $TrayOutputPath
$trayBytes = New-IconPng `
  -PathNodes $pathNodes `
  -Size 32 `
  -ViewBoxWidth $viewBox[2] `
  -ViewBoxHeight $viewBox[3] `
  -Pen $pen `
  -CropToVisibleBounds
$trayOutputDirectory = Split-Path -Parent $resolvedTrayOutputPath
New-Item -ItemType Directory -Path $trayOutputDirectory -Force | Out-Null
[System.IO.File]::WriteAllBytes($resolvedTrayOutputPath, $trayBytes)

$header = [System.IO.File]::ReadAllBytes($resolvedOutputPath)
$count = [BitConverter]::ToUInt16($header, 4)
if ($header.Length -lt 6 -or $count -ne $sizes.Count) {
  throw "Generated ICO does not contain the expected multi-size image set."
}

Write-Output "Generated $resolvedOutputPath from $resolvedSvgPath"
Write-Output ("ICO sizes: " + (($sizes | ForEach-Object { "${_}x${_}" }) -join ", "))
Write-Output "Generated $resolvedTrayOutputPath (32x32 cropped tray PNG)"
Write-Output "Foreground: #000000; background: transparent; render stroke: ${renderStrokeWidth}px"
