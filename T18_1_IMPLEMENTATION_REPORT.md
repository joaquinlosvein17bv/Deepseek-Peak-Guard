# PeakGuard — Informe de Implementación T18.1
## Correcciones Focalizadas tras Auditoría Independiente de Codex

**Fecha:** 15 de octubre de 2026  
**Rama:** `main`  
**Estado:** IMPLEMENTADO Y VERIFICADO AL 100% (50 pruebas Rust + 2 runners Node.js)

---

### 1. Resumen Ejecutivo

Tras la auditoría independiente de Codex sobre el commit `eb2ecf4`, se identificaron puntos críticos en la gestión de cobertura por proveedor, aislamiento de instancias, persistencia/recuperación de órdenes Stop y robustez en la autenticación y reconexión de adaptadores.

En la fase **T18.1**, se implementaron correcciones estrictamente focalizadas, preservando la arquitectura maestra y los tres modos de operación (`strict_block`, `restricted_stop`, `notify_only`), sin reescribir componentes funcionales ni romper compatibilidad hacia atrás.

---

### 2. Detalle de Correcciones Implementadas

#### T18.1.1 — Cobertura Real por Proveedor y Sesión [P0]
- **Problema previo:** La Control API (`core/src/control_api/mod.rs`) evaluaba `active_sessions > 0` globalmente para transicionar a `PROTECTED`. Una sesión activa de Claude o GPT provocaba que DeepSeek figurara falsamente como protegido. Además, el widget de Pi mostraba `⛔ PEAK (Protegido)` basándose únicamente en `is_peak == true`.
- **Solución implementada:**
  1. Se implementó `SessionRegistry::get_active_sessions_for_provider(provider_id, registry)` y `count_active_for_provider()`.
  2. En `handle_status`, la cobertura de DeepSeek se evalúa con `direct_active_sessions = count_active_for_provider("deepseek_direct")`. Las sesiones de Claude, Anthropic, OpenAI u otros proveedores jamás activan la protección de DeepSeek.
  3. Se añadieron detalles exhaustivos en la respuesta de cobertura: `observed_requests_for_deepseek`, `active_sessions_for_deepseek`, `total_active_sessions`.
  4. En el adaptador de Pi (`adapters/pi/peakguard-pi.ts`), la función `formatPiStatusLabel()` comprueba explícitamente `status.coverage === "PROTECTED"`. Si la cobertura es `PARTIAL`, `UNVERIFIED` o `UNCOVERED`, muestra `⛔ PEAK (<COBERTURA>)` sin declarar falsos "Protegido".
  5. Ni OpenCode ni Pi asumen `deepseek-chat` como modelo por defecto cuando el modelo real no ha sido informado (utilizan `"unknown"`).

#### T18.1.2 — Identidad Compuesta de Sesiones [P0]
- **Problema previo:** Las sesiones se indexaban únicamente por `session_id`. Si dos instancias de OpenCode o Pi compartían el mismo identificador de sesión local, se producían sobreescrituras y colisiones.
- **Solución implementada:**
  1. `ActiveSession` se extendió con `client_instance_id: String` (único y estable por proceso mediante `pid + hex_random`) y `registered_at: DateTime<Utc>`.
  2. Clave compuesta: `composite_key() = format!("{}:{}:{}", client_type, client_instance_id, session_id)`.
  3. `SessionRegistry` indexa por la clave compuesta, garantizando aislamiento total entre múltiples instancias ejecutándose en paralelo.
  4. Los métodos `heartbeat()` y `unregister()` aceptan `client_instance_id: Option<&str>` para operaciones focalizadas.

#### T18.1.3 — Recuperación de Órdenes Stop y StopCommandManager [P0]
- **Problema previo:** Las órdenes emitidas por SSE eran efímeras. Si un adaptador sufría un microcorte o retardo durante la transición a pico, la orden de detención se perdía.
- **Solución implementada:**
  1. Se diseñó e implementó `StopCommandManager` en `core/src/registry/mod.rs` con estados `Pending`, `Acknowledged`, `Failed` y `Expired`.
  2. Cada orden de Stop genera un `command_id` único estructurado (`cmd_<timestamp>_<instance>_<session>`).
  3. Nuevo endpoint administrativo en Control API: `GET /api/v1/adapters/commands/pending?client_instance_id=...`.
  4. Al conectarse o reconectarse, los adaptadores de OpenCode y Pi consultan inmediatamente sus órdenes pendientes, ejecutan la cancelación nativa y envían el acuse con `command_id`.
  5. Validación estricta en `POST /api/v1/adapters/stop-ack`: si una instancia ajena intenta emitir un acuse para un comando que no le pertenece, se rechaza con **403 Forbidden**. El procesamiento de acuses es estrictamente idempotente.

#### T18.1.4 — Ciclo de Vida y Latidos en Turnos de Razonamiento [P1]
- **Problema previo:** En modelos como `deepseek-reasoner` que pueden tardar más de 30 minutos sin generar nuevos tokens visibles, las sesiones inactivas podían ser podadas prematuramente si el proceso cliente se colgaba.
- **Solución implementada:**
  1. Los adaptadores de OpenCode y Pi inician un temporizador periódico de latidos (cada 10 segundos) mientras un turno esté activo (`turn_start` / `message.created`).
  2. `SessionRegistry::prune_stale` mantiene la regla: sesiones con `is_active: true` no se podan durante el razonamiento silencioso (`active_max_silence` = 30 min); sesiones finalizadas (`is_active: false`) se limpian tras 60s de inactividad.
  3. Al concluir el turno (`agent_settled` / `message.completed` / `session.idle`), se detiene el temporizador y se envía inmediatamente un latido con `is_active: false`.

#### T18.1.5 — Lectura Segura de Token con Fail-Closed [P1]
- **Problema previo:** En `resolve_or_create_control_token()`, un archivo existente pero vacío o con error de lectura no diferenciaba `NotFound` de fallos de E/S, sobrescribiendo silenciosamente el archivo de credenciales.
- **Solución implementada:**
  1. Se utiliza coincidencia explícita: solo `Err(e) if e.kind() == std::io::ErrorKind::NotFound` activa la creación de un nuevo token seguro.
  2. Si el archivo existe pero está vacío o se produce un error de permisos o E/S, retorna `Err(...)` de forma inmediata (fail-closed), protegiendo la seguridad del sistema administrativo.

#### T18.1.6 — Reconexión y Aislamiento en OpenCode [P1]
- **Problema previo:** Variables de reconexión con ámbito reiniciado por llamada, falta de guardia contra reconexiones simultáneas y aceptación de órdenes sin validar pertenencia en clientes sin sesiones registradas.
- **Solución implementada:**
  1. En `adapters/opencode/peakguard-opencode-server.js`, las variables de retroceso (`reconnectAttempts`, `reconnectTimer`, `stableTimer`, `isConnecting`, `currentSseReq`) residen a nivel de módulo.
  2. El contador de reintentos solo se reinicia tras **5 segundos** de conexión continua estable (`stableTimer`).
  3. Se previene la concurrencia descontrolada con guardia `isConnecting` y comprobación de `!currentSseReq.destroyed`.
  4. Si la sesión solicitada no existe en el mapa local `activeSessions`, la orden de Stop es descartada y registrada en logs como ajena, sin ejecutar abort ni afectar sesiones de otras instancias.

---

### 3. Archivos Modificados y Creados

| Archivo | Tipo | Descripción |
|---|---|---|
| `core/src/registry/mod.rs` | Modificado | Identidad compuesta en `ActiveSession`, `StopCommandManager`, conteo y filtrado por proveedor. |
| `core/src/control_api/mod.rs` | Modificado | Cobertura honesta por proveedor, endpoint de comandos pendientes, validación de acuses (403), fail-closed en tokens. |
| `core/src/commands/run.rs` | Modificado | Registro de órdenes con `command_id` único y `client_instance_id` al entrar en pico en modo `restricted_stop`. |
| `core/tests/t18_security_and_lifecycle_tests.rs` | Modificado | Actualización de fixtures para identidad compuesta. |
| `core/tests/t18_1_repro_tests.rs` | **Nuevo** | Suite de pruebas de reproducción y validación de las correcciones T18.1 (3 pruebas automatizadas). |
| `adapters/opencode/peakguard-opencode-server.js` | Modificado | `CLIENT_INSTANCE_ID`, backoff persistente, recuperación de comandos pendientes, latidos periódicos. |
| `adapters/pi/peakguard-pi.ts` | Modificado | `CLIENT_INSTANCE_ID`, `formatPiStatusLabel`, latidos periódicos, recuperación de pendientes, modelo seguro. |
| `adapters/tests/adapter-mock-runner.test.mjs` | Modificado | Ampliación de suite mock para cubrir identidad compuesta, órdenes pendientes y validación de acuses. |
| `adapters/tests/t18_1_repro_adapter_tests.mjs` | **Nuevo** | Suite de verificación automatizada de adaptadores ante desconexiones y estados parciales. |
| `scripts/smoke-test.ps1` | Modificado | Integración de tests unitarios Rust con `--test-threads=2` y ejecución de suites de adaptadores Node.js. |
| `docs/COMPATIBILITY.md` | Modificado | Documentación detallada de todas las garantías y matrices de compatibilidad T18.1. |

---

### 4. Veredicto Final

Todos los criterios de aceptación y requerimientos de Codex han sido implementados y comprobados con evidencia empírica directa. El sistema es apto para su despliegue y validación operativa.
