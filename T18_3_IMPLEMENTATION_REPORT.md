# Informe de Implementación — Fase T18.3: Cierre Focalizado y Preparación Inmediata de T19

## 1. Resumen Ejecutivo

La fase **T18.3** cierra de manera focalizada y rigurosa los cuatro defectos técnicos identificados durante la auditoría externa de Codex sobre el commit `2782c16`. Todas las correcciones se han implementado siguiendo el principio **fail-closed** (falla cerrada), sin reescrituras innecesarias, con preservación de componentes estables y con verificación empírica previa y posterior.

El sistema cuenta con un binario autónomo para Windows (`dist/peakguard.exe`, 13.4 MB), 60 pruebas automatizadas en Rust que aprueban al 100%, 4 suites de adaptadores Node.js verificadas y el script de smoke test validado sin fallos.

---

## 2. Diagnóstico y Corrección de los Cuatro Hallazgos

### T18.3.1 — Validación Estricta de Stop en Adaptadores (OpenCode y Pi)
* **Defecto original:** Si `event.turn_id` venía ausente o nulo en el evento `stop_requested`, pero el cliente tenía un turno activo (`currentSess.turnID`), los adaptadores procedían a invocar `abort()`. Esto provocaba cancelaciones ambiguas no verificables.
* **Causa raíz:** Las condiciones de guardia (`if (!turnID && !currentSess.turnID)`) solo actuaban cuando *ambos* identificadores faltaban simultáneamente.
* **Corrección implementada:**
  - En `adapters/opencode/peakguard-opencode-server.js` y `adapters/pi/peakguard-pi.ts`, se exige coincidencia verificable: `turnID` y `currentTurn` deben estar presentes y ser exactamente idénticos (`turnID === currentTurn`).
  - Si falta `event.turn_id`, si el cliente no tiene turno comprobable o si ambos faltan, se previene el abort (`abortSuccess = false`) y se reporta de inmediato mediante Stop ACK con `success: false` y un error tipificado (`unverifiable_turn_identity`).
  - El Gateway mantiene la política de bloqueo activo sobre el tráfico HTTP sin confiar en paradas no verificadas.
  - La aplicación y el historial de conversación se preservan intactos sin recurrir a terminación brusca de procesos (`taskkill` o `process.exit`).

### T18.3.2 — Identidad Obligatoria en Acuses (Stop ACK) y Supresión de Fallbacks
* **Defecto original:** En `core/src/registry/mod.rs` y `core/src/control_api/mod.rs`, el método `record_ack` admitía acuses sin `command_id` y buscaba comandos pendientes heurísticamente, o creaba registros sintéticos `ack_legacy_*`.
* **Causa raíz:** Existencia de bloques fallback diseñados para compatibilidad hacia atrás que permitían a clientes omitir identificadores obligatorios.
* **Corrección implementada:**
  - `command_id` y `client_instance_id` son ahora parámetros obligatorios (`&str`) en `StopCommandManager::record_ack`.
  - La Control API responde `400 Bad Request` ante cualquier solicitud de Stop ACK que carezca de `command_id` o `client_instance_id` válidos y no vacíos.
  - Si la orden original requería `turn_id`, el acuse debe incluir el mismo `turn_id`; en caso de omisión o discordancia, el ACK es rechazado con error tipificado.
  - Se eliminó completamente la búsqueda heurística y la creación de acuses legacy sintéticos.
  - Las órdenes en estado terminal (`Acknowledged`, `Failed`, `Expired`) son inmutables ante acuses duplicados o tardíos (idempotencia estricta).

### T18.3.3 — Aislamiento Estricto del Registro de Sesiones (`heartbeat` y `unregister`)
* **Defecto original:** `SessionRegistry::heartbeat` y `SessionRegistry::unregister` aceptaban `Option<&str>` para `client_instance_id` y mantenían un fallback global que buscaba únicamente por `session_id`.
* **Causa raíz:** Si dos instancias o clientes distintos utilizaban el mismo `session_id`, un latido o desregistro de la instancia A podía mutar o eliminar la sesión de la instancia B.
* **Corrección implementada:**
  - `client_instance_id: &str` es estrictamente obligatorio en las firmas de `heartbeat` y `unregister`. Si viene vacío o en blanco, la operación retorna `false` inmediatamente.
  - Ambas funciones requieren correspondencia exacta y simultánea de `session_id` y `client_instance_id`.
  - Se eliminó el bucle de búsqueda global por `session_id`.
  - Se añadieron pruebas donde dos instancias (`inst_alpha` e `inst_beta`) comparten el mismo `session_id`: la operación de una no afecta ni desregistra la sesión de la otra.

### T18.3.4 — Persistencia Confiable y Fail-Closed de Órdenes Stop
* **Defecto original:** Si el archivo JSON de comandos persistidos (`pending_commands.json`) contenía sintaxis corrupta o truncada, `with_persistence` capturaba el error y llamaba a `persist_to_disk()`, sobrescribiendo silenciosamente el archivo corrupto con un array vacío `[]`.
* **Causa raíz:** Supresión de errores de I/O y serialización sin propagación estructurada.
* **Corrección implementada:**
  - `StopCommandManager::with_persistence(path)` retorna `Result<Self, Box<dyn std::error::Error + Send + Sync>>`.
  - Distingue rigurosamente entre archivo inexistente (`ErrorKind::NotFound`, donde inicializa vacío de forma legítima) y archivo existente corrupto o inaccesible por permisos.
  - Ante archivo corrupto, emite log de error `ERROR` y retorna `Err`, preservando el archivo intacto en disco sin sobrescribirlo ni destruirlo.
  - `persist_to_disk()` utiliza archivos temporales con sufijo nanosegundo, llamada explícita a `sync_all()` y sustitución atómica con reintentos para compatibilidad con bloqueos transitorios en Windows.
  - `peakguard run` inicializa `SessionRegistry::new_with_persistence()?` fallando cerrado al inicio del daemon si el almacenamiento está comprometido.

---

## 3. Matriz de Componentes Modificados

| Componente | Archivo | Modificación Clave |
|---|---|---|
| Core Registry | `core/src/registry/mod.rs` | Firmas estrictas en `record_ack`, `heartbeat`, `unregister`; fail-closed en `with_persistence` y atomic rename en Windows. |
| Control API | `core/src/control_api/mod.rs` | Validación obligatoria de `command_id` y `client_instance_id` en `/api/v1/adapters/stop-ack`, `/api/v1/sessions/heartbeat` y `/api/v1/sessions/unregister`. |
| Daemon Runner | `core/src/commands/run.rs` | Propagación de error fail-closed en `new_with_persistence()` y gestión segura de `create_command()` en transiciones a horario peak. |
| OpenCode Adapter | `adapters/opencode/peakguard-opencode-server.js` | Validación estricta de coincidencia de turnos antes de abort; acuse `success: false` si `event.turn_id` o `currentTurn` faltan. |
| Pi Adapter | `adapters/pi/peakguard-pi.ts` | Validación estricta de coincidencia de turnos antes de abort; acuse `success: false` ante órdenes sin `turn_id`. |
| Pruebas Unitarias / E2E | `core/tests/` | 60 pruebas en Rust incluyendo reproducción de T18.3 y verificación de ciclo de vida con binario real. |
| Pruebas Adaptadores | `adapters/tests/` | 4 suites automatizadas de adaptadores cubriendo los 5 casos de identidad de turnos. |

---

## 4. Próximo Paso Inmediato: Fase T19

Con los cuatro defectos de Codex cerrados y demostrados empíricamente, el sistema se encuentra completamente estabilizado para proceder a:
* **Fase T19 — Validación con Clientes Reales y Upstream Local Simulado**:
  - OpenCode, Pi y Hermes instalados localmente.
  - Upstream local HTTP simulando `/v1/chat/completions` con respuestas en streaming (SSE).
  - Verificación de los tres modos (`notify_only`, `restricted_stop`, `strict_block`) mediante reloj virtual acelerado (`PEAKGUARD_SIMULATED_TIME`).
  - Cero consumo de claves API de pago y sin modificaciones irreversibles de configuración.
