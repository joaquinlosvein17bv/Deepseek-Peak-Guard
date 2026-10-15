# Smoke test de verificación automática para PeakGuard
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$coreDir = Join-Path $root "core"
$bin = Join-Path $coreDir "target\x86_64-pc-windows-gnu\release\peakguard.exe"

if (-not (Test-Path $bin)) {
    $bin = Join-Path $coreDir "target\x86_64-pc-windows-gnu\debug\peakguard.exe"
}

if (-not (Test-Path $bin)) {
    throw "Error: No se encontró peakguard.exe para ejecutar smoke test."
}

Write-Host "==> Ejecutando Smoke Test contra: $bin" -ForegroundColor Cyan

# 1. Version test
Write-Host "1. Probando comando 'version'..."
& $bin version
if ($LASTEXITCODE -ne 0) { throw "Fallo en 'version'" }

# 2. Config validate test
Write-Host "2. Probando comando 'config validate'..."
$cfg = Join-Path $root "config\peakguard.example.toml"
& $bin config validate $cfg
if ($LASTEXITCODE -ne 0) { throw "Fallo en 'config validate'" }

# 3. Doctor test
Write-Host "3. Probando comando 'doctor'..."
& $bin doctor --config $cfg
if ($LASTEXITCODE -ne 0) { throw "Fallo en 'doctor'" }

# 4. Cargo tests
Write-Host "4. Ejecutando suite completa de pruebas unitarias y de integración..."
if (Test-Path "$env:USERPROFILE\scoop\apps\mingw\current\bin") {
    $env:PATH = "$env:USERPROFILE\scoop\apps\mingw\current\bin;" + $env:PATH
}
Set-Location $coreDir
cargo test
if ($LASTEXITCODE -ne 0) { throw "Fallo en cargo test" }

Set-Location $root
Write-Host "==> ¡Smoke test completado exitosamente! Todos los componentes funcionan correctamente." -ForegroundColor Green
