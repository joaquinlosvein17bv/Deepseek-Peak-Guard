# Build & Packaging script for PeakGuard (Windows x64)
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$coreDir = Join-Path $root "core"
$distDir = Join-Path $root "dist"

Write-Host "==> Configurando entorno de compilación..." -ForegroundColor Cyan
if (Test-Path "$env:USERPROFILE\scoop\apps\mingw\current\bin") {
    $env:PATH = "$env:USERPROFILE\scoop\apps\mingw\current\bin;" + $env:PATH
}

Write-Host "==> Compilando PeakGuard Release en $coreDir..." -ForegroundColor Cyan
Set-Location $coreDir
cargo build --release

$binPath = Join-Path $coreDir "target\x86_64-pc-windows-gnu\release\peakguard.exe"
if (-not (Test-Path $binPath)) {
    throw "Error: No se encontró el binario compilado en $binPath"
}

Write-Host "==> Preparando carpeta de distribución en $distDir..." -ForegroundColor Cyan
if (Test-Path $distDir) {
    Remove-Item -Recurse -Force $distDir
}
New-Item -ItemType Directory -Path $distDir | Out-Null

Copy-Item $binPath $distDir
Copy-Item (Join-Path $root "config\peakguard.example.toml") $distDir
Copy-Item (Join-Path $root "README.md") $distDir

$distBin = Join-Path $distDir "peakguard.exe"
$hash = (Get-FileHash -Algorithm SHA256 $distBin).Hash
$checksumFile = Join-Path $distDir "checksums.sha256"
"$hash  peakguard.exe" | Out-File -Encoding ASCII $checksumFile

$sizeMb = [math]::Round((Get-Item $distBin).Length / 1MB, 2)
Write-Host "==> ¡Empaquetado completado exitosamente!" -ForegroundColor Green
Write-Host "    Binario: $distBin ($sizeMb MB)"
Write-Host "    SHA-256: $hash"
Set-Location $root
