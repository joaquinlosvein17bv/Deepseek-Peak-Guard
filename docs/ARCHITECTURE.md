# Arquitectura Técnica de PeakGuard

**Versión:** 1.0  
**Fecha:** 14 de octubre de 2026

---

## 1. Visión General de la Arquitectura

PeakGuard está diseñado como un **demonio local determinista y ultra-ligero** escrito en Rust, complementado por adaptadores por cliente y un servidor MCP opcional.

```text
 ┌────────────────────────────────────────────────────────┐
 │                   Clientes Locales                     │
 │   - OpenCode (Plugin TypeScript / TUI)                 │
 │   - Pi Coding Agent (Extensión TypeScript / UI)        │
 │   - Hermes Agent (Config YAML / CLI)                   │
 └────────────────────────────────────────────────────────┘
          │                                  │
          │ Tráfico de inferencia            │ Señales de sesión,
          │ HTTP (model="deepseek...")       │ Stop nativo y estado
          ▼                                  ▼
 ┌─────────────────────────┐       ┌───────────────────────────┐
 │ Local HTTP Gateway      │       │ Local Control API & SSE   │
 │ (127.0.0.1:8765)        │       │ (127.0.0.1:8766 + Token)  │
 └─────────────────────────┘       └───────────────────────────┘
              │                                  │
              ▼                                  ▼
 ┌────────────────────────────────────────────────────────┐
 │                   Núcleo PeakGuard                     │
 │  - Clock: UTC, Test Clock, Jump Detector               │
 │  - Schedule Engine: Intervalos, Festivos               │
 │  - Model Registry: Proveedores, Aliases                │
 │  - Policy Engine: ALLOW / BLOCK / STOP_REQUIRED        │
 │  - Session Registry: Mapa de sesiones activas          │
 │  - Notification Engine: Deduplicación, Desktop, TG     │
 │  - Storage: TOML config, Atomic JSON state             │
 └────────────────────────────────────────────────────────┘
          │                                  │
          │ (Si ALLOW)                       │ (Opcional stdio)
          ▼                                  ▼
 ┌─────────────────────────┐       ┌───────────────────────────┐
 │ Upstream APIs           │       │ Servidor MCP Consultivo   │
 │ - api.deepseek.com      │       │ (tools: status, next_peak)│
 │ - opencode.ai/zen/go    │       └───────────────────────────┘
 └─────────────────────────┘
```

---

## 2. Puertos y Separación de Planos

| Plano | Endpoint por Defecto | Protocolo | Autenticación | Función |
|---|---|---|---|---|
| **Data Plane (Gateway)** | `127.0.0.1:8765` | HTTP / SSE | Auth de Upstream en header passthrough | Recibe llamadas OpenAI-compatible (`/v1/chat/completions`), evalúa política e intercepta tráfico en peak |
| **Control Plane (API)** | `127.0.0.1:8766` | HTTP / SSE | Bearer Token local obligatorio (`PEAKGUARD_CONTROL_TOKEN`) | Registro de adaptadores, acuse de Stop (`stop-ack`), eventos SSE y cambio de modo |
| **MCP Plane** | `stdio` | JSON-RPC 2.0 | Proceso hijo local | Herramientas consultivas para agentes |

---

## 3. Máquina de Estados del Policy Engine

El motor de políticas calcula deterministamente una de cuatro decisiones por cada solicitud o transición:
1. `ALLOW`: La solicitud procede y es retransmitida de forma transparente al upstream.
2. `BLOCK`: La solicitud es rechazada inmediatamente en el gateway local con HTTP 403 estructurado (`peakguard_policy_blocked`). Cero bytes salen hacia internet.
3. `STOP_REQUIRED`: Se rechaza cualquier solicitud nueva en gateway y se emite un evento de Stop nativo (`session.abort()` o `ctx.abort()`) hacia el adaptador que posee la sesión activa.
4. `NOTIFY`: Se permite el tráfico, pero se activan las alertas programadas (T−30, T−10, T−5, T−10s).

### Comportamiento según el Modo Configurado

| Modo | Fuera de Peak | Ventana Preventiva (T−30 a T−1) | Inicio de Peak (T=0) | Durante Peak |
|---|---|---|---|---|
| `strict_block` | `ALLOW` | Alertas previas + cuenta regresiva | `BLOCK` inmediato de nuevas peticiones y cancelación de streams | `BLOCK` continuo |
| `restricted_stop` | `ALLOW` | Alertas previas + cuenta regresiva | `STOP_REQUIRED` en adaptador + `BLOCK` en gateway | `BLOCK` continuo; Stop no se repite |
| `notify_only` | `ALLOW` | Alertas previas + cuenta regresiva | Alerta de inicio de peak; tráfico `ALLOW` | `ALLOW` continuo |

---

## 4. Estructura Canónica de Errores del Gateway

Cuando una solicitud es bloqueada por política horaria, el gateway responde con código HTTP 403 (para evitar que SDKs reintenten infinitamente como sucedería con 429 o 503) y el siguiente cuerpo JSON:

```json
{
  "error": {
    "code": "peakguard_policy_blocked",
    "message": "Petición restringida por política horaria de PeakGuard (horario peak activo).",
    "provider_id": "deepseek_direct",
    "model_id": "deepseek-chat",
    "mode": "restricted_stop",
    "current_time_utc": "2026-10-19T01:05:00Z",
    "next_off_peak_utc": "2026-10-19T04:00:00Z",
    "retry_automatically": false
  }
}
```

---

## 5. Eventos SSE del Plano de Control (`GET /api/v1/events`)

Los adaptadores escuchan eventos del plano de control en tiempo real mediante Server-Sent Events (SSE):

- `event: state_changed`: Notifica cambio entre `OFF_PEAK`, `PREPARING`, `COUNTDOWN`, `PEAK`, `BLOCKED`.
- `event: stop_requested`: Ordena al adaptador cancelar el turno de una sesión específica:
  ```json
  {
    "session_id": "sess-xyz-123",
    "provider_id": "deepseek_direct",
    "model_id": "deepseek-chat",
    "transition_utc": "2026-10-19T01:00:00Z"
  }
  ```
- `event: countdown_tick`: Emite los segundos restantes durante la cuenta regresiva final (10 s a 1 s).
