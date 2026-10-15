# Informe de Implementación Técnica — Fase T18.2: Corrección Integral de Hallazgos BLOCKED de Codex

**Fecha:** 15 de octubre de 2026  
**Rama:** `main`  
**Estado:** COMPLETO Y VERIFICADO (100% Tests Pasados: 55 Rust + 4 Suites Node.js)  
**Entorno:** Windows 11 Pro Build 26200.0 x64, Rust 1.99.0, Node.js v24.13.1  

---

## 1. Resumen Ejecutivo

La fase **T18.2** corrige exhaustiva y focalizadamente los cinco problemas arquitectónicos identificados por la auditoría independiente de Codex sobre el commit `441c6b2`. Todos los cambios respetan rigurosamente la especificación maestra, preservan los tres modos de operación (`strict_block`, `restricted_stop`, `notify_only`) y mantienen la separación estricta entre la intermediación de tráfico del Gateway HTTP y la API administrativa de Control.

---

## 2. Detalle de Correcciones por Hallazgo

### T18.2.1 — Corrección de Falsa Cobertura PROTECTED [P0]
- **Problema previo:** `core/src/control_api/mod.rs` permitía que la Control API reportara `coverage: "PROTECTED"` si `direct_active_sessions > 0`, incluso cuando el gateway no había intermediado ninguna petición real. Asimismo, los adaptadores de OpenCode y Pi asignaban `deepseek_direct` por defecto para modelos desconocidos.
- **Acción correctiva implementada:**
  1. En `core/src/control_api/mod.rs`: se eliminó `direct_active_sessions > 0` como condición de concesión de `PROTECTED`. Ahora se exige estrictamente `direct_traffic_observed == true` (tráfico loopback real observado e intermediado por el Gateway 8765). Una sesión de DeepSeek registrada sin tráfico observado reporta `PARTIAL` con razón `"session_registered_awaiting_gateway_traffic"`.
  2. En `adapters/opencode/peakguard-opencode-server.js` y `adapters/pi/peakguard-pi.ts`: se eliminó `deepseek_direct` como proveedor por defecto. Cuando el modelo o proveedor es desconocido, se reporta neutralmente como `"unknown"`.
  3. En `core/tests/t18_1_repro_tests.rs`: se corrigió la prueba para validar que una sesión activa sin tráfico inicia en `PARTIAL` y solo transiciona a `PROTECTED` tras `record_observed_traffic`.

### T18.2.2 — Prevención de Stop de Turnos Equivocados [P0]
- **Problema previo:** Una orden Stop tardía emitida para un turno anterior (`turn-1`) provocaba que los adaptadores abortaran un nuevo turno activo en curso (`turn-2`), al no comparar `event.turn_id` contra el turno activo del cliente.
- **Acción correctiva implementada:**
  1. En `adapters/opencode/peakguard-opencode-server.js`: en `handleStopOrder`, se valida `currentSess.turnID !== event.turn_id`. Si difiere, la orden es descartada sin invocar `client.session.abort({ path })`, registrando advertencia y emitiendo un Stop ACK con `success: false, error: "turn_mismatch"`.
  2. En `adapters/pi/peakguard-pi.ts`: se valida `event.turn_id !== activeTurnId`. Si no coincide, se descarta la orden tardía sin llamar a `abort()`, emitiendo Stop ACK con `turn_mismatch`.
  3. En ambos adaptadores: si la identidad del turno es inverificable o no hay turno activo (`!currentSess.isActive` / `!activeTurnId`), se evita un abort ambiguo y se delega de forma segura en el gateway loopback.

### T18.2.3 — Validación Estricta de Stop ACK [P0]
- **Problema previo:** `StopAckRequest` permitía omitir `client_instance_id`. Además, no se verificaba el `turn_id` contra el comando original, y `handle_stop_ack` evaluaba `payload.success` en lugar del estado real del comando, permitiendo que un segundo ACK mutara un comando en estado terminal `Failed` a `Acknowledged`.
- **Acción correctiva implementada:**
  1. En `core/src/control_api/mod.rs`: `client_instance_id` es obligatorio; si falta o está vacío, responde inmediatamente con código `400 BAD_REQUEST` (`"missing_client_instance_id"`).
  2. En `core/src/registry/mod.rs` (`StopCommandManager::record_ack`):
     - Valida `client_instance_id` (403 Forbidden si proviene de una instancia ajena).
     - Valida `session_id` y `turn_id` (400 Bad Request si difieren del comando registrado).
     - **Idempotencia terminal estricta:** si el comando ya se encuentra en `Acknowledged`, `Failed` o `Expired`, retorna el comando existente sin modificar sus campos de acuse ni su estado.
  3. En `core/src/control_api/mod.rs`: la respuesta HTTP refleja el estado terminal inmutable de `recorded_cmd.status`:
     - `Acknowledged` -> `200 OK`, `{"status": "acknowledged", "success": true}`.
     - `Failed` -> `202 ACCEPTED`, `{"status": "failed", "success": false}`.
     - `Expired` -> `202 ACCEPTED`, `{"status": "expired", "success": false}`.

### T18.2.4 — Persistencia y Recuperación de Órdenes tras Reinicio del Daemon [P1]
- **Problema previo:** Si el daemon de PeakGuard se reiniciaba durante horario pico con turnos pendientes de cancelación, el registro en memoria perdía las órdenes pendientes, dejando desprotegida la sesión.
- **Acción correctiva implementada:**
  1. En `core/src/registry/mod.rs`: se implementó persistencia atómica en disco (`~/.peakguard/pending_commands.json`, configurable mediante la variable de entorno `PEAKGUARD_COMMANDS_PATH`). La escritura se realiza mediante un archivo temporal con sincronización a disco (`sync_all()`) y reemplazo atómico (`rename`), con permisos Unix `0600`.
  2. **Reconciliación en arranque:** al inicializarse, carga las órdenes existentes. Si una orden pendiente tiene más de 10 minutos de antigüedad (`created_at > 10m`), pasa automáticamente a `StopCommandStatus::Expired` con error explicativo. Las órdenes pendientes recientes (<10m) se restauran como `Pending`. Las órdenes terminales conservan su estado inmutable.
  3. En `core/src/commands/run.rs`: el demonio arranca con `SessionRegistry::new_with_persistence()`. Si arranca durante horario peak, el monitor de transiciones y el gateway loopback aplican inmediatamente la política de bloqueo/stop a cualquier petición o sesión activa.

### T18.2.5 — Reconexión Estable y Backoff en Pi [P2]
- **Problema previo:** En `adapters/pi/peakguard-pi.ts`, la conexión SSE reseteaba `reconnectAttempts = 0` tan pronto como recibía `res.statusCode === 200`, lo que ante micro-cortes o desconexiones instantáneas causaba un bucle rápido de reconexión sin backoff.
- **Acción correctiva implementada:**
  1. En `adapters/pi/peakguard-pi.ts`: se introdujo `stableTimer` de 5.000 ms. Solo tras 5 segundos de conexión continua ininterrumpida se restablece `reconnectAttempts = 0`. Si la conexión cae antes de los 5 segundos, el contador de reintentos se incrementa manteniendo el backoff exponencial.
  2. Se añadió bandera de protección `isConnecting` y destrucción garantizada de sockets previos (`sseReq.destroy()`) para evitar bucles concurrentes de reconexión.

---

## 3. Matriz de Archivos Modificados

| Componente | Archivo Modificado | Naturaleza del Cambio |
|---|---|---|
| **Core (Control API)** | `core/src/control_api/mod.rs` | Exigencia obligatoria de `client_instance_id` (400), respuesta inmutable según estado terminal de comandos, cobertura PROTECTED requiere tráfico gateway real |
| **Core (Registry)** | `core/src/registry/mod.rs` | Persistencia atómica de órdenes (`pending_commands.json`), reconciliación de órdenes expiradas (>10m), validación estricta de turnos e idempotencia terminal |
| **Core (Daemon)** | `core/src/commands/run.rs` | Inicialización de `SessionRegistry` con persistencia en disco |
| **Core (Tests)** | `core/tests/t18_2_repro_tests.rs` | Suite de pruebas de reproducción T18.2.1, T18.2.3 y T18.2.4 |
| **Core (Tests)** | `core/tests/t18_1_repro_tests.rs` | Ajuste de aserción para cobertura PARTIAL sin tráfico |
| **Core (Tests)** | `core/tests/real_daemon_e2e_tests.rs` | Incorporación de `client_instance_id` obligatorio en peticiones stop-ack |
| **OpenCode Adapter** | `adapters/opencode/peakguard-opencode-server.js` | Validación de turnos `turnID !== currentSess.turnID`, proveedor `unknown` neutral |
| **Pi Adapter** | `adapters/pi/peakguard-pi.ts` | Temporizador de estabilidad (5s), validación estricta de turnos, proveedor `unknown` neutral |
| **Adapter Tests** | `adapters/tests/t18_2_repro_adapter_tests.mjs` | Suite de pruebas de adaptadores para turnos discordantes y proveedores neutros |
| **Documentación** | `docs/COMPATIBILITY.md` | Actualización de matriz de compatibilidad a versión T18.2 |
