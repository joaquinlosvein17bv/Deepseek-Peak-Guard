# PeakGuard CLI Monitor para Hermes Workspaces
$ErrorActionPreference = "SilentlyContinue"
$url = if ($env:PEAKGUARD_CONTROL_URL) { $env:PEAKGUARD_CONTROL_URL } else { "http://127.0.0.1:8766" }

try {
    $resp = Invoke-RestMethod -Uri "$url/api/v1/status" -TimeoutSec 2
    if ($resp) {
        $icon = if ($resp.is_peak) { "[PEAK ACTIVO]" } else { "[OFF-PEAK]" }
        $remaining = if ($resp.seconds_to_peak) { [math]::Floor($resp.seconds_to_peak / 60).ToString() + "m" } else { "N/A" }
        Write-Host "PeakGuard: $icon Modo: $($resp.mode) | Proximo: $remaining | Cobertura: $($resp.coverage)" -ForegroundColor $(if ($resp.is_peak) { "Red" } else { "Green" })
    }
} catch {
    Write-Host "PeakGuard: [Daemon Inactivo en $url]" -ForegroundColor DarkGray
}
