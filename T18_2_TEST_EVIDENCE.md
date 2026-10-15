# Registro de Evidencias de Pruebas — Fase T18.2: Correcciones BLOCKED de Codex

**Fecha de ejecución:** 15 de octubre de 2026  
**Resultado Global:** 100% SATISFACTORIO (0 fallos, 0 errores)  
**Entorno:** Windows 11 Build 26200.0 x64, Rust 1.99.0, Node.js v24.13.1  

---

## 1. Resumen Consolidado de Pruebas

| Suite / Ejecutable de Prueba | Tipo de Validación | Tests Ejecutados | Resultado |
|---|---|:---:|:---:|
| `core/src/lib.rs` (unittests) | Config, Registry, Deduplicación | 10 | **PASÓ (10/10)** |
| `core/src/main.rs` (unittests) | Binario, CLI y Config | 10 | **PASÓ (10/10)** |
| `tests/gateway_e2e_tests.rs` | Gateway HTTP loopback y bloqueo | 1 | **PASÓ (1/1)** |
| `tests/gateway_streaming_tests.rs` | Concurrencia SSE y 20 streams paralelos | 1 | **PASÓ (1/1)** |
| `tests/mcp_server_tests.rs` | Protocolo MCP, tools y modo read-only | 8 | **PASÓ (8/8)** |
| `tests/policy_mode_tests.rs` | Modos de operación y límites de horario | 7 | **PASÓ (7/7)** |
| `tests/real_daemon_e2e_tests.rs` | Puertos reales (8765/8766), auth y ciclo de vida | 2 | **PASÓ (2/2)** |
| `tests/schedule_boundary_tests.rs` | Bordes horarios, reloj simulado y festivos | 4 | **PASÓ (4/4)** |
| `tests/stream_cancellation_tests.rs` | Cancelación de streams activos en peak | 2 | **PASÓ (2/2)** |
| `tests/t18_1_repro_tests.rs` | Cobertura aislada por proveedor y auth fail-closed | 3 | **PASÓ (3/3)** |
| `tests/t18_2_repro_tests.rs` | **Falsa cobertura, Stop ACK estricto y persistencia** | 3 | **PASÓ (3/3)** |
| `tests/t18_security_and_lifecycle_tests.rs` | CSPRNG, SSE Bearer header, turnos largos | 6 | **PASÓ (6/6)** |
| **Total Rust Core** | | **57** | **100% OK** |
| `adapters/tests/t18_2_repro_adapter_tests.mjs` | Turnos discordantes y proveedores neutros | 4 | **PASÓ** |
| `adapters/tests/adapter-mock-runner.test.mjs` | Aislamiento, mock server, abort nativo OpenCode/Pi | 8 | **PASÓ** |
| `adapters/tests/t18_1_repro_adapter_tests.mjs` | Cobertura honesta en UI de Pi y recuperación | 2 | **PASÓ** |
| `scripts/smoke-test.ps1` | Pipeline completo de verificación E2E | Completo | **PASÓ** |
| `scripts/build-windows.ps1` | Compilación Release y empaquetado de distribución | Completo | **PASÓ** |

---

## 2. Evidencia de la Suite T18.2 en Rust (`t18_2_repro_tests.rs`)

```text
running 3 tests
test test_repro_session_registration_alone_must_not_grant_protected_coverage ... ok
test test_repro_stop_ack_must_require_mandatory_client_instance_and_prevent_terminal_mutation ... ok
test test_repro_stop_recovery_after_daemon_restart ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

### Comportamientos específicos validados:
1. `test_repro_session_registration_alone_must_not_grant_protected_coverage`:
   - Registra una sesión activa de DeepSeek (`session_id = "session-ds-no-traffic"`, `provider_id = "deepseek_direct"`).
   - Verifica que `GET /api/v1/status` devuelve `coverage = "PARTIAL"` (nunca `PROTECTED`).
   - Invoca `record_observed_traffic("deepseek_direct", "deepseek-chat")`.
   - Verifica que solo al registrar tráfico real intermediado por el gateway se reporta `PROTECTED`.
2. `test_repro_stop_ack_must_require_mandatory_client_instance_and_prevent_terminal_mutation`:
   - Crea orden de detención `cmd` para instancia `inst_valid_01` y turno `turn-10`.
   - Envía Stop ACK sin `client_instance_id` -> Recibe `400 Bad Request` (`"missing_client_instance_id"`).
   - Envía Stop ACK con `turn_id: "turn-99-wrong"` -> Rechazado con `400 Bad Request`.
   - Envía Stop ACK con `success = false` -> Registrado como estado terminal `Failed` (`202 Accepted`).
   - Intenta enviar segundo Stop ACK con `success = true` -> Responde `202 Accepted` conservando inmutable el estado terminal `failed`, impidiendo cualquier mutación fraudulenta.
3. `test_repro_stop_recovery_after_daemon_restart`:
   - Inicializa `SessionRegistry` con persistencia en `pending_commands.json`.
   - Crea orden reciente (<10m) y orden antigua (>10m).
   - Simula apagado y reinicio completo del daemon (`drop(reg1)` y creación de `reg2` sobre el mismo archivo).
   - Verifica que la orden reciente se recupera en estado `Pending`.
   - Verifica que la orden antigua se reconcilia automáticamente a estado `Expired` con motivo explicativo.
   - Procesa Stop ACK exitoso para la orden reciente -> Pasa a `Acknowledged`.
   - Simula un segundo reinicio (`drop(reg2)` y `reg3`) -> Verifica que el estado terminal `Acknowledged` persiste en disco.

---

## 3. Evidencia de la Suite de Adaptadores T18.2 (`t18_2_repro_adapter_tests.mjs`)

```text
=== Reproducción de defectos T18.2 en adaptadores ===
[TEST REPRO T18.2.1] Proveedor registrado por defecto para sesión sin proveedor: 'unknown'
[TEST REPRO T18.2.2] Emitiendo orden Stop tardía para 'turn-1' cuando la sesión ya está en 'turn-2'...
[TEST REPRO T18.2.2] abortCalled reportado tras orden tardía: false
--- Pruebas de Pi para T18.2 ---
[TEST REPRO T18.2.1 Pi] Proveedor para modelo 'unknown': 'unknown'
[TEST REPRO T18.2.2 Pi] Emitiendo orden Stop tardía para turn-1 a Pi...
[PeakGuard:Pi] Orden Stop descartada por discordancia de turno: la orden es para 'turn-1', pero el turno actual activo es 'turn-2'.
[TEST REPRO T18.2.2 Pi] piAbortCalled: false
  ✓ Pi envió Stop ACK con error 'turn_mismatch' y no abortó el nuevo turno.
=== Todas las pruebas de adaptadores T18.2 completadas con éxito ===
```

### Comportamientos específicos validados en adaptadores:
1. **OpenCode:**
   - Sesión sin proveedor/modelo registra `"unknown"`, evitando falsas atribuciones a DeepSeek.
   - Sesión en `turn-2` ante orden tardía para `turn-1` no invoca `client.session.abort()`.
2. **Pi:**
   - Modelo `"unknown"` registra `provider_id = "unknown"`.
   - Turno en `turn-2` ante orden tardía para `turn-1` no invoca `pi.abort()`.
   - Emite acuse Stop ACK informando `success: false, error: "turn_mismatch"`.
   - Temporizador de estabilidad de reconexión SSE configurado en 5.000 ms.

---

## 4. Evidencia de Smoke Test (`scripts/smoke-test.ps1`)

```text
==> ¡Smoke test completado exitosamente! Todos los componentes funcionan correctamente.
```
- Validó compilación unitaria de `core`.
- Validó todas las pruebas de integración en Rust.
- Validó los runners de adaptadores en Node.js sin fallos.

---

## 5. Evidencia del Empaquetado de Distribución Windows (`scripts/build-windows.ps1`)

```text
==> Configurando entorno de compilación...
==> Compilando PeakGuard Release en core...
   Compiling peakguard-core v0.1.0
    Finished `release` profile [optimized] target(s) in 38.94s
==> Preparando carpeta de distribución en dist...
==> ¡Empaquetado completado exitosamente!
    Binario: dist\peakguard.exe (13.38 MB)
    SHA-256: 2DA9B4106447EA26FADED5BD2C085D86476084BD95C9797AACD5F88BAC33554B
```

---

## 6. Conclusión de la Auditoría T18.2

Los cinco hallazgos BLOCKED de Codex han sido corregidos con evidencia técnica demostrable y reproducible. No existen falsos positivos en cobertura ni en acuses de recibo. Los turnos en curso quedan garantizados frente a cancelaciones cruzadas y las órdenes de parada resisten reinicios del sistema.

PeakGuard se encuentra formalmente listo para la **Fase T19** (pruebas reales con OpenCode, Pi y Hermes mediante un upstream local simulado).
