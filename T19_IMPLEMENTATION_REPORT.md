# Informe de Implementación — Fase T19: Pruebas de Integración con Clientes Instalados y Upstream Simulado

## 1. Resumen Ejecutivo

La fase **T19** consolida la validación de extremo a extremo (E2E) de **PeakGuard** frente a los tres entornos de agentes de desarrollo soportados (**OpenCode**, **Pi Coding Agent** y **Hermes Agent**), ejecutándose contra el binario real `peakguard run` en Windows y un servidor upstream local simulado.

### Principios Cumplidos
1. **Cero consumo de claves API de pago:** Toda la inferencia fue intermediada hacia un servidor mock local HTTP en puertos efímeros, simulando tanto respuestas JSON estándar como Server-Sent Events (SSE) streaming.
2. **Preservación absoluta de aplicaciones y sesiones:** En ningún momento se utilizaron utilidades destructivas (`taskkill`, `process.kill` ni `process.exit()` en código de producción). Los turnos activos se abortaron exclusivamente mediante las APIs nativas de cada cliente (`client.session.abort()` en OpenCode y `ctx.abort()` en Pi).
3. **Simulación determinista del tiempo:** Se empleó la variable `PEAKGUARD_SIMULATED_TIME="2026-10-15T02:00:00Z"` para evaluar el comportamiento exacto durante la ventana oficial de horario pico de DeepSeek (01:00–04:00 UTC).
4. **Verificación de los tres modos operativos:** Se demostró la conmutación dinámica en caliente entre `notify_only`, `restricted_stop` y `strict_block` sin requerir reinicio del demonio.

---

## 2. Arquitectura de Integración T19

```
┌────────────────────────────────────────────────────────────────────────┐
│                        SUITE DE INTEGRACIÓN T19                        │
└────────────────────────────────────────────────────────────────────────┘
          │                                            │
          ▼                                            ▼
┌──────────────────┐                         ┌──────────────────┐
│  Hermes Agent    │                         │ OpenCode Server  │
│  (Data Plane GW) │                         │ (Control API/SSE)│
└─────────┬────────┘                         └─────────┬────────┘
          │                                            │
          │ HTTP Requests                              │ SSE Events / REST
          ▼                                            ▼
┌───────────────────────────────────────────────────────────────┐
│               PeakGuard Daemon (peakguard.exe)                │
│                                                               │
│  Gateway HTTP (Puerto efímero)  Control API (Puerto efímero)  │
│  - Allowlist / Filtrado horario  - Registro central de sesión  │
│  - Corte de stream SSE en peak   - Emisión de Stop / Stop ACK │
│  - Inyección de cabeceras        - Bearer Token CSPRNG        │
└───────────────┬───────────────────────────────────────────────┘
                │ Proxy de Tráfico
                ▼
┌───────────────────────────────────────────────────────────────┐
│              Mock Upstream Server (Localhost)                 │
│  - /v1/chat/completions (JSON chat completions)               │
│  - /chat/completions (SSE text/event-stream chunks + [DONE])   │
└───────────────────────────────────────────────────────────────┘
```

---

## 3. Resultados Detallados por Componente

### 3.1. Hermes Agent — Integración vía Data Plane (Gateway HTTP)
* **Comportamiento en Horario Peak (`restricted_stop` y `strict_block`):**
  - Una petición enviada por Hermes hacia `http://127.0.0.1:<gw_port>/v1/chat/completions` con `model: "deepseek-chat"` es interceptada en el Gateway y rechazada con **HTTP 403 Forbidden**.
  - La carga de respuesta contiene la estructura oficial: `{"error":{"code":"peakguard_policy_blocked","message":"...horario peak..."}}`.
  - El cliente HTTP de Hermes recibe el error limpiamente y aborta la generación sin que el proceso principal de Hermes ni sus subprocesos de delegación sufran terminación forzada.
* **Comportamiento Off-Peak y Modelos Independientes:**
  - Peticiones hacia modelos independientes o rutas off-peak (como `deepseek-v4.1-flash` configurado para `opencode_go`) reciben **HTTP 200 OK** con la respuesta generada por el upstream simulado.

### 3.2. OpenCode Adapter (`peakguard-opencode-server.js`)
* **Registro de Sesión y Latidos:**
  - El adaptador establece conexión SSE con `GET /api/v1/events` autenticado mediante cabecera Bearer Token.
  - Registra sesiones activas compuestas con `session_id`, `client_instance_id`, `turn_id`, `model` y `provider_id`.
  - Envía latidos periódicos (`POST /api/v1/sessions/heartbeat`) confirmando que la sesión y el turno de razonamiento continúan vivos.
* **Validación Estricta de Órdenes Stop (T18.3.1):**
  - Ante una orden `stop_requested` con `turn_id: null` o ambiguo, el adaptador previene el abort (`abortSuccess = false`) y emite un acuse Stop ACK con `success: false` y error tipificado `unverifiable_turn_identity`. El gateway mantiene el bloqueo de seguridad.
  - Ante una orden legítima con coincidencia verificable de instancia, sesión y turno, el adaptador invoca `client.session.abort({ path: { id: sessionID } })` y envía un Stop ACK con `success: true` y `command_id`.
  - El proceso OpenCode no se cierra y el historial de chat permanece intacto.
  - Al finalizar, la sesión es desregistrada mediante `POST /api/v1/sessions/unregister`.

### 3.3. Pi Coding Agent (`peakguard-pi.ts`)
* **Widget UI y Cobertura Confiable (T18.1.1):**
  - La función `getPeakGuardStatus()` consulta `GET /api/v1/status`.
  - `formatPiStatusLabel()` muestra el estado fidedigno (ej. `🛡️ OFF-PEAK (--)`) y **jamás reporta "Protegido"** mientras el tráfico observado no confirme cobertura `PROTECTED`.
* **Ciclo de Vida y Abort Nativo:**
  - El evento `turn_start` registra el turno activo.
  - Al recibir una orden Stop coincidente, ejecuta `activeCtx.abort()` de manera limpia.
  - Reporta el acuse Stop ACK a la Control API con su `CLIENT_INSTANCE_ID` único.
  - El evento `agent_settled` emite latido con `is_active: false`.

### 3.4. Conmutación Dinámica de Modos y Streaming SSE
* **Modo `notify_only`:**
  - Conmutado en caliente vía `PUT /api/v1/config/mode` con Bearer Token.
  - Las peticiones a `deepseek-chat` en horario pico pasan al upstream simulado.
  - Peticiones con `stream: true` reciben cabecera `Content-Type: text/event-stream` y los fragmentos `chatcmpl-t19-mock-chunk-1`, `chatcmpl-t19-mock-chunk-2` y `[DONE]` sin cortes espurios.
* **Modo `strict_block`:**
  - Conmutado en caliente vía `PUT /api/v1/config/mode`.
  - Bloqueo instantáneo con HTTP 403 Forbidden para todos los modelos con calendario pico activo.

### 3.5. Apagado Ordenado del Demonio (`Shutdown`)
* La Control API recibe `POST /api/v1/control/shutdown` con autorización Bearer.
* El canal de cancelación finaliza el bucle de eventos, libera los sockets TCP de los puertos 8765 y 8766 y el proceso `peakguard.exe` finaliza con código de salida `0`.

---

## 4. Matriz de Entregables de la Fase T19

| Entregable | Ubicación | Descripción |
|---|---|---|
| Suite de Integración E2E en Node.js | `adapters/tests/t19_e2e_integration.test.mjs` | Prueba de ciclo de vida completo: daemon real + upstream mock + OpenCode + Pi + Hermes + 3 modos + streaming + shutdown. |
| Suite de Pruebas de Integración en Rust | `core/tests/t19_integration_tests.rs` | 3 pruebas de integración en Rust ejecutando el binario compilado contra upstream mock, streaming SSE y registro de sesiones/ACKs. |
| Exportación Limpia de Handlers en Adaptadores | `adapters/opencode/peakguard-opencode-server.js` y `adapters/pi/peakguard-pi.ts` | Exportación top-level de `handleStopOrder` y variables de sesión para pruebas aisladas y deterministas. |
| Empaquetado Windows Release | `dist/peakguard.exe` | Binario optimizado y verificado (13.4 MB, SHA-256 verificado). |
| Evidencia Técnica | `T19_TEST_EVIDENCE.md` | Trazas completas de ejecución, salidas de pruebas y diagnóstico de clientes instalados. |

---

## 5. Estado del Proyecto

Con la conclusión de la Fase T19, **PeakGuard ha alcanzado la madurez de integración de producción**:
- 63 pruebas automatizadas en Rust aprobadas sin fallos.
- 5 suites de adaptadores y pruebas E2E en Node.js aprobadas al 100%.
- Compatibilidad demostrada con las herramientas reales instaladas en la máquina del usuario (OpenCode, Pi y Hermes).
- Cero fugas de claves, cero terminaciones bruscas de procesos y fail-closed integral.
