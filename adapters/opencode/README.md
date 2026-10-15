# Adaptador PeakGuard para OpenCode

## Requisitos
- OpenCode v1.18+ (instalado en el equipo del usuario).
- PeakGuard daemon corriendo en segundo plano (`peakguard run`).

## Instalación en OpenCode

1. Copiar `peakguard-opencode.js` a la carpeta de plugins globales de OpenCode:
   ```powershell
   Copy-Item adapters/opencode/peakguard-opencode.js $env:USERPROFILE\.config\opencode\plugins\
   ```

2. Añadir la referencia en `$env:USERPROFILE\.config\opencode\opencode.jsonc`:
   ```jsonc
   "plugin": [
     "./plugins/peakguard-opencode.js"
   ]
   ```

3. (Opcional) Configurar el proveedor OpenAI-compatible para pasar inferencia por el Gateway en `127.0.0.1:8765`:
   ```json
   "provider": {
     "peakguard-deepseek": {
       "name": "PeakGuard DeepSeek",
       "npm": "@ai-sdk/openai-compatible",
       "options": {
         "baseURL": "http://127.0.0.1:8765/v1"
       }
     }
   }
   ```

## Funcionamiento
- Cuando PeakGuard entra en horario Peak en modo `restricted_stop`, envía un evento de cancelación por el canal SSE.
- El plugin recibe la orden e invoca nativamente `client.session.abort({ path: { id: sessionID } })`.
- El turno activo se detiene de forma limpia, la ventana de la TUI de OpenCode permanece abierta y el historial se preserva íntegramente.
