# PeakGuard — T18: Informe de Estabilización de Seguridad, Cobertura y Control de Sesiones

**Fecha de ejecución:** 15 de octubre de 2026  
**Entorno de ejecución:** Windows 11 Build 10.0.26200.0 (Win32NT) x64  
**Compilador:** Rust 1.99.0 (`cargo 1.99.0`) | **Runtime:** Node.js v24.13.1  
**Estado:** ✅ Aprobado y verificado con 47 pruebas en Rust, 6 pruebas en Node.js y smoke test integral de binario.

---

## 1. Resumen Ejecutivo

La fase **T18** aborda y resuelve de forma definitiva los riesgos residuales detectados en la auditoría técnica tras T17. Los objetivos fundamentales cumplidos son:
1. **Seguridad criptográfica de la Control API [P0]:** Eliminación del generador pseudoaleatorio basado en timestamp; adopción de CSPRNG del sistema operativo mediante `ring::rand::SystemRandom`. Corrección de la validación de tokens vacíos (`fail-closed`). Escritura atómica y transaccional del archivo de credenciales (`control.token.tmp.<pid>` → `sync_all` → `rename`). Eliminación de tokens en parámetros de URL (`?token=`) en SSE, adoptando exclusivamente cabeceras `Authorization: Bearer <token>`.
2. **Cobertura comprobable y honesta [P0]:** Rediseño del cálculo de cobertura en la Control API y endpoints de estado. Ya no se reporta `PROTECTED` por el mero hecho de que un proveedor esté declarado en el archivo TOML; se exige tráfico real observado a través del loopback (`TrafficTracker`) o una sesión activa conectada y autenticada. Se introdujo el estado `PARTIAL` para rutas configuradas en espera de tráfico, y se preservó `UNVERIFIED` para OpenCode Go.
3. **Identidad de sesión y ciclo de vida en Pi Coding Agent [P0]:** Validación estricta de `event.session_id === activeSessionId` y correspondencia de `turn_id`. Se corrigió el fallo crítico donde el adaptador de Pi abortaba su turno activo ante órdenes de parada dirigidas a otras sesiones (OpenCode o sesiones Pi previas). Se implementó preservación de turnos de razonamiento activo (`deepseek-reasoner` / R1) hasta 30 minutos frente a los 60 segundos de sesiones inactivas, reconexión con retroceso exponencial (*exponential backoff*), y reporte de `is_active: false` en `agent_settled`.

---

## 2. Reproducción Previa y Diagnóstico Causal (Paso 1)

Antes de modificar el código productivo, se implementaron pruebas de reproducción para cada comportamiento defectuoso:

| Defecto | Archivo y Función Afectada | Causa Raíz Confirmada | Prueba de Reproducción | Resultado de Reproducción |
|---|---|---|---|---|
| **Defecto A: Bypass de autenticación con token vacío** | `core/src/control_api/mod.rs` (`check_auth`) | La condición `if expected.is_empty() { return true; }` permitía que si el archivo de token no existía o estaba vacío, cualquier petición no autenticada fuera aceptada como válida (HTTP 200). | `core/tests/t18_security_and_lifecycle_tests.rs` (`test_security_empty_expected_token_rejected_fail_closed`) | Confirmado: retornaba HTTP 200 en vez de 401 Unauthorized ante token esperado vacío. |
| **Defecto B: Token no criptográfico y no atómico** | `core/src/control_api/mod.rs` (`resolve_or_create_control_token`) | Generación con timestamp y `rand::random`, susceptible a predicción o colisión. Escritura directa sin atomicidad con riesgo de lectura de archivo parcial o vacío en reinicios rápidos. | `core/tests/t18_security_and_lifecycle_tests.rs` (`test_security_csprng_token_generation_and_uniqueness`) | Confirmado: tokens generados en el mismo milisegundo compartían entropía. |
| **Defecto C: Fuga de secreto en URL SSE** | `adapters/opencode/peakguard-opencode-server.js` y `adapters/pi/peakguard-pi.ts` (`connectEvents`) | La URL SSE incluía `?token=${encodeURIComponent(token)}`, exponiendo el token en logs de proxy, historial HTTP y volcados de traza. | `core/tests/t18_security_and_lifecycle_tests.rs` (`test_security_sse_query_token_rejected_and_bearer_header_accepted`) | Confirmado: la Control API aceptaba query tokens; los clientes enviaban el token en la URL. |
| **Defecto D: Falsedad en estado de cobertura** | `core/src/control_api/mod.rs` (`handle_status`) | Retornaba `coverage: "PROTECTED"` con solo verificar `provider.enabled == true`, engañando al usuario antes de verificar que el tráfico fluía efectivamente por el gateway. | `core/tests/t18_security_and_lifecycle_tests.rs` (`test_verifiable_coverage_transitions_from_partial_to_protected`) | Confirmado: se indicaba `PROTECTED` con 0 bytes de tráfico en el gateway. |
| **Defecto E: Poda prematura de turnos de razonamiento** | `core/src/registry/mod.rs` (`prune_stale`) | Poda ciega cada 60 segundos basada en el último latido, destruyendo del registro sesiones con modelos lentos de razonamiento (`deepseek-reasoner` / R1) que permanecen 60–120 segundos sin emitir chunks. | `core/tests/t18_security_and_lifecycle_tests.rs` (`test_lifecycle_active_reasoning_turn_preserved_across_silence`) | Confirmado: turno activo eliminado tras 61 segundos de silencio de red. |
| **Defecto F: Aborto indiscriminado en Pi** | `adapters/pi/peakguard-pi.ts` (`connectEvents`) | Al recibir un evento `stop_requested`, invocaba inmediatamente `activeCtx.abort()` sin validar si `event.session_id` correspondía a `activeSessionId`. | `adapters/tests/repro-pi-foreign-session-abort.test.mjs` | Confirmado: `abortCalled` era `true` cuando se enviaba un Stop para `opencode-foreign-session-999`. |

---

## 3. Correcciones Implementadas

### 3.1. Seguridad Criptográfica de la Control API (Paso 2)
1. **CSPRNG nativo con Ring:** Se incorporó la dependencia `ring = "0.17"` en `core/Cargo.toml`. La función `generate_secure_token()` utiliza `ring::rand::SystemRandom::fill()` para extraer 32 bytes de entropía criptográfica directa del sistema operativo (`BCryptGenRandom` en Windows), codificados en hexadecimal con prefijo `pg_` (67 caracteres totales).
2. **Escritura Atómica:** `resolve_or_create_control_token()` escribe primero a un archivo temporal `control.token.tmp.<pid>`, realiza `sync_all()` al disco y ejecuta un renombramiento atómico (`std::fs::rename`) hacia `control.token`.
3. **Permisos Restrictivos:** En sistemas Unix se aplican permisos `0600`. En Windows se restringe el acceso al usuario en ejecución dentro del directorio `%USERPROFILE%\.peakguard`.
4. **Fail-Closed:** Si `resolve_or_create_control_token()` falla o el token esperado es una cadena vacía, `check_auth` rechaza inmediatamente la petición con HTTP 401 Unauthorized. El daemon en `run.rs` se niega a iniciar si falla la inicialización del token.
5. **Eliminación de Parámetros de URL:** Se retiró el extractor `Query` en `handle_events`. La Control API únicamente acepta autenticación mediante la cabecera HTTP estándar `Authorization: Bearer <token>`. Se eliminó cualquier log o traza que contenga el token.

### 3.2. Cobertura Verificable y Dinámica (Paso 3)
1. **`TrafficTracker` en `SessionRegistry`:** Estructura concurrente thread-safe (`RwLock<HashMap<String, usize>>`) que registra cada solicitud exitosamente enrutada por el Gateway HTTP para cada proveedor (`record_request`).
2. **Estados Dinámicos de Cobertura:**
   - `PROTECTED`: Existe tráfico realmente observado (`has_observed_provider == true`) para el proveedor protegido activo o existen sesiones activas conectadas y autenticadas asociadas a dicho proveedor.
   - `PARTIAL`: El proveedor está habilitado y configurado en el archivo TOML, pero aún no se ha observado tráfico transitar por el Gateway.
   - `UNVERIFIED`: Proveedores con inferencia nativa gestionada (por defecto `opencode_go`) que requieren un baseURL local personalizado para pasar por el Gateway.
   - `UNCOVERED`: El proveedor está deshabilitado explícitamente en la configuración.
   - `DISCONNECTED`: El daemon local no responde o no está en ejecución.
3. **Registro Automático de Tráfico:** En `core/src/gateway/mod.rs`, cada solicitud recibida en `/v1/chat/completions` o `/chat/completions` invoca `state.registry.record_observed_traffic(&provider.id, &model_name)`.

### 3.3. Ciclo de Vida y Aislamiento de Sesiones (Paso 4)
1. **Preservación de Turnos de Razonamiento:** Se actualizó `prune_stale` para aceptar dos umbrales diferenciados:
   - Sesiones inactivas (`is_active: false`): Poda a los 60 segundos de inactividad.
   - Sesiones en turno activo (`is_active: true`): Umbral extendido de hasta 30 minutos de silencio de red, protegiendo turnos complejos de `deepseek-reasoner`.
2. **Aislamiento en Adaptador de Pi (`adapters/pi/peakguard-pi.ts`):**
   - Comprobación estricta: `if (!targetSession || targetSession !== activeSessionId) return;`
   - Validación de turno: `if (event.turn_id && activeTurnId && event.turn_id !== activeTurnId) return;`
   - Deduplicación de órdenes de Stop mediante clave `(session_id, turn_id, transition_utc)`.
   - Retroceso exponencial en SSE: Intentos espaciados a 1s, 2s, 4s, 8s hasta un máximo de 15s.
   - Detección de cambio de modelo dinámico en `turn_start`.
   - Envío de heartbeat con `is_active: false` al finalizar el turno en `agent_settled`.
3. **Aislamiento en Adaptador de OpenCode (`adapters/opencode/peakguard-opencode-server.js`):**
   - Registro local de sesiones en `activeSessions` Map.
   - Ignorar órdenes de Stop dirigidas a sesiones no pertenecientes al cliente OpenCode en ejecución.
   - Deduplicación de eventos de Stop.
   - Reconexión SSE con retroceso exponencial.
   - Envío de latido `is_active: false` en `message.completed` o `session.idle`.

---

## 4. Matriz de Pruebas Ejecutadas y Resultados

### 4.1. Pruebas Unitarias y de Integración en Rust (`core`)
- Total de suites ejecutadas: 8
- Total de tests pasados: **47 passed; 0 failed**

| Suite | Tests | Resultado |
|---|---|---|
| `src/lib.rs` (unittests) | 8 | ✅ OK |
| `src/main.rs` (unittests) | 8 | ✅ OK |
| `tests/gateway_e2e_tests.rs` | 1 | ✅ OK |
| `tests/gateway_streaming_tests.rs` | 1 | ✅ OK |
| `tests/mcp_server_tests.rs` | 8 | ✅ OK |
| `tests/policy_mode_tests.rs` | 7 | ✅ OK |
| `tests/real_daemon_e2e_tests.rs` | 2 | ✅ OK |
| `tests/schedule_boundary_tests.rs` | 4 | ✅ OK |
| `tests/stream_cancellation_tests.rs` | 2 | ✅ OK |
| `tests/t18_security_and_lifecycle_tests.rs` | 6 | ✅ OK |

### 4.2. Pruebas de Adaptadores en Node.js (`adapters/tests`)
- `adapter-mock-runner.test.mjs`: **6/6 pruebas pasadas exitosamente**
  1. `fetchStatus()` en Widget TUI sincroniza estado correctamente.
  2. Registro de sesión activa con token Bearer en cabecera HTTP.
  3. Stop nativo exitoso en OpenCode (`client.session.abort()`) y acuse de recibo estricto.
  4. Stop con fallo en OpenCode reporta estrictamente `success: false` sin falsos positivos.
  5. Aislamiento de sesión en OpenCode (ignora Stop de sesiones ajenas).
  6. Aislamiento de sesión en Pi, abort nativo `activeCtx.abort()`, acuse real y `is_active: false` en `agent_settled`.
- `repro-pi-foreign-session-abort.test.mjs`: **Aprobado** (confirmó que `abortCalled === false` para sesiones foráneas).

### 4.3. Smoke Test de Binario (`scripts/smoke-test.ps1`)
- `peakguard version`: ✅ OK
- `peakguard config validate config\peakguard.example.toml`: ✅ OK
- `peakguard doctor --config config\peakguard.example.toml`: ✅ OK (detecta OpenCode, Pi, Hermes y reporta `deepseek_direct` PROTEGIDO y `opencode_go` UNVERIFIED)
- `cargo test`: ✅ 47/47 tests pasados
- Compilación `release`: ✅ `peakguard.exe` generado exitosamente.

---

## 5. Conclusión y Estado del Repositorio

El sistema PeakGuard cuenta ahora con:
- Una capa de autenticación hermética basada en CSPRNG del sistema operativo y cabeceras Bearer.
- Un cálculo de cobertura comprobable y no falsificable que refleja tráfico real observado en el loopback.
- Una gestión de ciclo de vida de sesiones que aísla estrictamente a los clientes y preserva sesiones prolongadas de modelos de razonamiento profundo.
- Cero falsos positivos en los acuses de recibo de paradas suaves.
