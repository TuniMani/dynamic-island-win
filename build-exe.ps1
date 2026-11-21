param(
    [switch]$RustOnly
)

$ErrorActionPreference = "Stop"
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

if (-not $RustOnly) {
    Write-Host "[1/3] Frontend derleniyor (npm.cmd run build)..." -ForegroundColor Cyan
    Push-Location $root
    try {
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "[1/3] -RustOnly aktif, frontend derlemesi atlandi." -ForegroundColor Yellow
}

Write-Host "[2/3] Rust release derleniyor (cargo build --release)..." -ForegroundColor Cyan
Push-Location (Join-Path $root "src-tauri")
try {
    & cargo build --release --features tauri/custom-protocol
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$srcExe = Join-Path $root "src-tauri\target\release\dynamic-island-win.exe"
$dstExe = Join-Path $root "DynamicIsland.exe"

Write-Host "[3/3] DynamicIsland.exe kopyalaniyor..." -ForegroundColor Cyan
$running = Get-Process -Name "DynamicIsland","dynamic-island-win" -ErrorAction SilentlyContinue
if ($running) {
    Write-Warning "DynamicIsland calisiyor; guncelleme icin once kapatiliyor..."
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 350
}

Copy-Item $srcExe $dstExe -Force
$sw.Stop()
$item = Get-Item $dstExe
$mb = [Math]::Round($item.Length / 1MB, 2)
Write-Host ("Hazir: {0} ({1} MB) - Sure: {2:N1} sn" -f $dstExe, $mb, $sw.Elapsed.TotalSeconds) -ForegroundColor Green
