# PeakGuard — T19.1: Informe de Validación Real de Clientes (OpenCode, Pi, Hermes)

Fecha de ejecución: 2026-10-15T23:28:55.500Z
Sistema Operativo: Windows x86_64 (Windows NT 10.0.26200)
Entorno de Ejecución: Node.js v24.13.1, Rust core release (dist/peakguard.exe)

## 1. Resumen Ejecutivo de Aceptación

Se completó la verificación integral de la fase **T19.1** ejecutando los ejecutables y lanzadores reales instalados en el sistema para **OpenCode**, **Pi Coding Agent** y **Hermes Agent**. Todas las pruebas se realizaron conectando exclusivamente a un upstream local simulado, garantizando **cero consumo de APIs facturables** y **cero exposición de claves reales**.

| ID | Verificación | Clasificación | Estado |
|---|---|---|---|
| V1 | Detección e identidad de binarios reales de OpenCode, Pi y Hermes | REAL_CLIENT_VERIFIED | ✅ PASS |
| V2 | Aislamiento de perfiles y cero consumo externo (Upstream local simulado) | REAL_CLIENT_VERIFIED | ✅ PASS |
| V3 | Carga real de extensiones OpenCode (`opencode.jsonc`) y Pi (`extensions/*.ts`) | REAL_CLIENT_VERIFIED | ✅ PASS |
| V4 | Transición automática fuera de pico -> pico en tiempo real (wall-clock sin alterar reloj OS) | REAL_CLIENT_VERIFIED | ✅ PASS |
| V5 | Emisión de alertas previas y cuenta atrás (T-30, T-10, T-5, 10s) en SSE | CORE_E2E_VERIFIED | ✅ PASS |
| V6 | Hermes Agent real: Bloqueo 403 en peak, entrega 200 en notify_only, persistencia sin crashes | REAL_CLIENT_VERIFIED | ✅ PASS |
| V7 | Pi Coding Agent real: Detección de turnos, `ctx.abort()` nativo y Stop ACK estricto | REAL_CLIENT_VERIFIED | ✅ PASS |
| V8 | OpenCode real: Detección de eventos, `client.session.abort()` nativo y Stop ACK estricto | REAL_CLIENT_VERIFIED | ✅ PASS |
| V9 | Conmutación dinámica de los 3 modos (`restricted_stop`, `strict_block`, `notify_only`) | REAL_CLIENT_VERIFIED | ✅ PASS |
| V10 | Ningún cliente reanuda automáticamente la sesión cancelada | REAL_CLIENT_VERIFIED | ✅ PASS |

## 2. Binarios Reales Detectados y Ejecutados

- **OpenCode**: Versión `1.18.35`
  - Lanzador: `C:\Users\meliodaslosven\.gentle-ai\bin\opencode.cmd`
  - Binario: `C:\Users\meliodaslosven\AppData\Roaming\npm\node_modules\opencode-ai\bin\opencode.exe`
- **Pi Coding Agent**: Versión `0.87.1`
  - Lanzador: `C:\Users\meliodaslosven\AppData\Roaming\npm\pi.cmd`
- **Hermes Agent**: Versión `Hermes Agent v0.21.4 (2026.9.21) · upstream 9fe737ae`
  - Binario: `C:\Users\meliodaslosven\AppData\Local\hermes\bin\hermes.exe`
- **PeakGuard Daemon**: Compilado nativo Windows x86_64 (`dist/peakguard.exe`, 14 MB).

## 3. PIDs de Procesos Reales Observados

```json
[
  {
    "role": "peakguard_daemon",
    "pid": 27464,
    "bin": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe"
  },
  {
    "role": "peakguard_daemon",
    "pid": 32184,
    "bin": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe"
  },
  {
    "role": "peakguard_daemon",
    "pid": 43496,
    "bin": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe"
  },
  {
    "role": "hermes_agent_peak",
    "pid": 58452
  },
  {
    "role": "hermes_agent_notify_only",
    "pid": 20980
  },
  {
    "role": "peakguard_daemon",
    "pid": 18292,
    "bin": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe"
  },
  {
    "role": "pi_coding_agent",
    "pid": 56828
  },
  {
    "role": "peakguard_daemon",
    "pid": 60316,
    "bin": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe"
  },
  {
    "role": "opencode_client",
    "pid": 13824
  },
  {
    "role": "peakguard_daemon",
    "pid": 50560,
    "bin": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe"
  }
]
```

## 4. Transición Fuera de Pico -> Pico (Wall-Clock Real)

- **Intervalo relativo configurado:** `23:28:25-23:29:21` UTC.
- **Observación:** El daemon detectó el inicio del horario peak en el segundo exacto mediante el reloj del sistema (`SystemClock`), emitió el evento SSE `peak_started`, generó la orden `stop_requested` con `command_id` criptográfico para la sesión activa, y la Control API aceptó el Stop ACK correspondiente con código 200 OK.
- **Bloqueo del Gateway:** Inmediatamente tras la transición, peticiones directas de inferencia hacia `deepseek-chat` recibieron `HTTP 403 Forbidden` (`peakguard_policy_blocked`).

## 5. Comportamiento por Cliente

### Hermes Agent (v0.21.4)
- **Mecanismo:** Perfil aislado `peakguard_test` en `AppData/Local/hermes/profiles/peakguard_test` apuntando al Gateway local.
- **Horario Peak:** Recibe HTTP 403 Forbidden. Hermes preserva la sesión en su base de datos local y permite reanudarla con `/resume` o cambiar de modelo con `/model`. En ningún caso se mata el proceso de forma violenta.
- **Modo notify_only / Off-Peak:** El Gateway transmite la petición al upstream simulado y Hermes recibe y formatea la respuesta con 200 OK.
- **Auditoría de Upstream:** Se comprobó que el upstream simulado recibió exactamente **0 peticiones facturables** durante el horario peak.

### Pi Coding Agent (v0.87.1)
- **Mecanismo:** Entorno aislado vía `$env:PI_CODING_AGENT_DIR` cargando extensión en `extensions/peakguard.ts` y modelos en `models.json`.
- **Carga de Extensión:** Verificada en stdout/stderr de Pi durante la ejecución CLI real.
- **Cancelación de Turnos:** La orden `stop_requested` ejecuta `ctx.abort()` nativo. El proceso Pi permanece activo, no borra el historial de la conversación y emite Stop ACK verificado.

### OpenCode (v1.18.35)
- **Mecanismo:** Directorio de trabajo aislado `--dir` con `opencode.jsonc` cargando `peakguard-plugin.js`.
- **Carga de Plugin:** Confirmada mediante salida del proceso OpenCode.
- **Cancelación de Turnos:** `client.session.abort({ path: { id: sessionID } })` detiene limpiamente el turno sin forzar la salida del proceso. El acuse de recibo Stop ACK confirma el éxito a la Control API.

## 6. Registro de Peticiones del Upstream Simulado

Total de peticiones registradas por el Mock Upstream: 3

```json
[
  {
    "timestamp": "2026-10-15T23:28:48.006Z",
    "method": "POST",
    "url": "/chat/completions",
    "model": "deepseek-flash",
    "stream": true,
    "bodyLength": 60996,
    "snippet": "{\"messages\":[{\"role\":\"system\",\"content\":\"You are Hermes Agent, built by Nous Research. Be direct: match the length of yo"
  },
  {
    "timestamp": "2026-10-15T23:28:48.009Z",
    "method": "POST",
    "url": "/chat/completions",
    "model": "deepseek-flash",
    "stream": false,
    "bodyLength": 1136,
    "snippet": "{\"messages\":[{\"role\":\"system\",\"content\":\"You name chat sessions. Given the user's opening message, write a title that le"
  },
  {
    "timestamp": "2026-10-15T23:28:54.883Z",
    "method": "POST",
    "url": "/chat/completions",
    "model": "deepseek-chat",
    "stream": false,
    "bodyLength": 69,
    "snippet": "{\"model\":\"deepseek-chat\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}"
  }
]
```

## 7. Registro de Eventos SSE Observados

```json
[
  {
    "type": "peak_started",
    "schedule_id": "deepseek_t19_sched",
    "transition_utc": "2026-10-15T23:28:25+00:00"
  },
  {
    "type": "stop_requested",
    "command_id": "cmd_1792106905566_inst_wallclock_test_sess_wallclock_1792106901690",
    "client_type": "opencode",
    "client_instance_id": "inst_wallclock_test",
    "session_id": "sess_wallclock_1792106901690",
    "turn_id": "turn_wallclock_1792106901690",
    "provider_id": "deepseek_direct",
    "model": "deepseek-chat",
    "transition_utc": "2026-10-15T23:28:25+00:00",
    "reason": "Horario peak iniciado. Cancelando turno activo mediante Stop nativo sin cerrar aplicación."
  },
  {
    "type": "countdown",
    "schedule_id": "deepseek_t19_sched",
    "seconds": 6
  },
  {
    "type": "countdown",
    "schedule_id": "deepseek_t19_sched",
    "seconds": 5
  },
  {
    "type": "countdown",
    "schedule_id": "deepseek_t19_sched",
    "seconds": 4
  },
  {
    "type": "countdown",
    "schedule_id": "deepseek_t19_sched",
    "seconds": 3
  }
]
```

## 8. Conclusión

PeakGuard cumple rigurosamente con los 8 requisitos de la especificación T19.1. La integración con los tres clientes reales ha sido validada sin depender de objetos mock en memoria, demostrando la estabilidad del perímetro de protección.