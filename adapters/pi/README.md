# Adaptador PeakGuard para Pi Coding Agent

## Requisitos
- Pi Coding Agent v0.87+ (instalado en el equipo del usuario).
- PeakGuard daemon corriendo en segundo plano (`peakguard run`).

## Instalación en Pi

1. Copiar el archivo de extensión a la carpeta de extensiones de Pi:
   ```powershell
   Copy-Item adapters/pi/peakguard-pi.ts $env:USERPROFILE\.pi\agent\extensions\
   ```

2. Configurar el proveedor OpenAI-compatible en `$env:USERPROFILE\.pi\agent\models.json`:
   ```json
   {
     "providers": {
       "deepseek-peakguard": {
         "baseUrl": "http://127.0.0.1:8765/v1",
         "api": "openai-completions",
         "apiKey": "$DEEPSEEK_API_KEY",
         "models": [
           {
             "id": "deepseek-chat",
             "name": "DeepSeek Chat (PeakGuard Protected)"
           }
         ]
       }
     }
   }
   ```

## Funcionamiento
- Pi muestra el estado en vivo en el status bar (`ctx.ui.setStatus`).
- Durante horario Peak, las nuevas peticiones se bloquean en el Gateway local y cualquier turno activo se interrumpe de forma limpia mediante `ctx.abort()`.
- La aplicación permanece abierta y el historial de la sesión se preserva intacto sin llamar a `ctx.shutdown()`.
