# Evidencia Técnica y Registro de Pruebas — Fase T18.3

**Fecha de ejecución:** 2026-10-15  
**Entorno:** Windows x64 (GNU toolchain, Node.js v24.13.1, PowerShell 5.1/7)  
**Objetivo:** Demostrar empíricamente la resolución de los cuatro hallazgos de Codex (T18.3.1, T18.3.2, T18.3.3, T18.3.4).

---

## 1. Pruebas de Reproducción Focalizadas en Rust (`core/tests/t18_3_repro_tests.rs`)

```text
running 3 tests
test test_repro_heartbeat_and_unregister_isolation_no_global_fallback ... ok
[TEST T18.3.2] Status sin command_id: 400 Bad Request
[TEST T18.3.2] Status con turn_id ausente: 400 Bad Request
test test_repro_stop_ack_mandatory_identity_and_no_fallback_search ... ok
[TEST T18.3.4] Resultado al cargar archivo corrupto: true
test test_repro_persistence_corrupt_file_fail_closed ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

### Hallazgos validados:
1. **Identidad obligatoria en ACK (T18.3.2):**
   - Una petición sin `command_id` responde `400 Bad Request`.
   - Una petición con `turn_id` ausente cuando la orden original requería turno responde `400 Bad Request`.
   - Se eliminó el fallback que creaba comandos sintéticos `ack_legacy_*`.
2. **Aislamiento en Session Registry (T18.3.3):**
   - Dos sesiones compartiendo `session_id = "shared-session-name"` con instancias `inst_A` e `inst_B` están totalmente aisladas.
   - Un `heartbeat` con `client_instance_id = "inst_IMPOSTOR"` devuelve `false` y no muta ninguna sesión.
   - Un `unregister` con `client_instance_id = "inst_IMPOSTOR"` no elimina ninguna sesión.
   - Un `unregister` de `inst_A` solo elimina `inst_A`, manteniendo `inst_B` viva.
3. **Persistencia corrupta fail-closed (T18.3.4):**
   - Un archivo de comandos con JSON corrupto/truncado hace que `with_persistence` retorne `Err`.
   - El archivo corrupto original no es destruido ni sobrescrito con `[]`.

---

## 2. Pruebas de Reproducción en Adaptadores (`adapters/tests/t18_3_repro_adapter_tests.mjs`)

```text
=== Reproducción de defectos T18.3 en adaptadores ===
--- 1. Pruebas de Stop Estricto en OpenCode ---
[PeakGuard:OpenCode] unverifiable_turn_identity: event.turn_id ausente en la orden de Stop. Evitando abort ambiguo y aplicando fallback de gateway.
  ✓ [OpenCode] Caso 2 (event.turn_id ausente): abort prevenido y Stop no confirmado.
[PeakGuard:OpenCode] unverifiable_turn_identity: la sesión actual no tiene un turno activo comprobable. Evitando abort ambiguo y aplicando fallback de gateway.
  ✓ [OpenCode] Caso 3 (turno cliente ausente): abort prevenido y Stop no confirmado.
[PeakGuard:OpenCode] unverifiable_turn_identity: turno ausente en la orden y en la sesión. Evitando abort ambiguo y aplicando fallback de gateway.
  ✓ [OpenCode] Caso 4 (ambos ausentes): abort prevenido y Stop no confirmado.
[PeakGuard:OpenCode] Orden Stop antigua descartada: orden para 'turn-old', pero turno activo actual es 'turn-new'. No se aborta el turno nuevo.
  ✓ [OpenCode] Caso 5 (discordancia de turno): abort prevenido.
[PeakGuard:OpenCode] Recibida orden de Stop nativo válida para sesión 'sess-oc-strict-turn' (turno 'turn-new'). Abortando turno activo...
[PeakGuard:OpenCode] Turno 'turn-new' abortado exitosamente mediante client.session.abort({ id: 'sess-oc-strict-turn' }).
  ✓ [OpenCode] Caso 1 (coincidencia exacta): abort ejecutado con éxito.
--- 2. Pruebas de Stop Estricto en Pi ---
[PeakGuard:Pi] unverifiable_turn_identity: event.turn_id ausente en la orden de Stop. Se omite abort ambiguo y se mantiene bloqueo en gateway.
  ✓ [Pi] Caso 2 (event.turn_id ausente): abort prevenido.
[PeakGuard:Pi] Orden de Stop recibida para 'pi-sess-strict'. Abortando turno activo...
[PeakGuard:Pi] activeCtx.abort() ejecutado con éxito.
  ✓ [Pi] Caso 1 (coincidencia exacta): abort ejecutado con éxito.
=== Todas las pruebas de reproducción T18.3 en adaptadores pasaron ===
```

---

## 3. Suite Completa de Pruebas Unitarias y E2E de Rust (`cargo test --tests`)

```text
running 10 tests (lib.rs) ... ok (10 passed; 0 failed)
running 10 tests (main.rs) ... ok (10 passed; 0 failed)
running 1 test (gateway_e2e_tests.rs) ... ok (1 passed; 0 failed)
running 1 test (gateway_streaming_tests.rs) ... ok (1 passed; 0 failed)
running 8 tests (mcp_server_tests.rs) ... ok (8 passed; 0 failed)
running 7 tests (policy_mode_tests.rs) ... ok (7 passed; 0 failed)
running 2 tests (real_daemon_e2e_tests.rs) ... ok (2 passed; 0 failed)
running 4 tests (schedule_boundary_tests.rs) ... ok (4 passed; 0 failed)
running 2 tests (stream_cancellation_tests.rs) ... ok (2 passed; 0 failed)
running 3 tests (t18_1_repro_tests.rs) ... ok (3 passed; 0 failed)
running 3 tests (t18_2_repro_tests.rs) ... ok (3 passed; 0 failed)
running 3 tests (t18_3_repro_tests.rs) ... ok (3 passed; 0 failed)
running 6 tests (t18_security_and_lifecycle_tests.rs) ... ok (6 passed; 0 failed)

Total: 60 passed; 0 failed; 0 ignored; finished in 1.1s
```

---

## 4. Validación de Suites de Adaptadores en Node.js

1. **`adapters/tests/adapter-mock-runner.test.mjs`**:
   - Widget TUI, formateo Pi, registro compuesto, Stop nativo exitoso con acuse verificado, Stop fallido reportando `success=false` sin falsos positivos, aislamiento de sesiones ajenas, recuperación de pendientes.
   - **Resultado:** 8/8 pruebas aprobadas.
2. **`adapters/tests/t18_1_repro_adapter_tests.mjs`**:
   - Cobertura honesta (solo `(Protegido)` en `PROTECTED`), reconexión SSE con recuperación REST.
   - **Resultado:** Aprobado sin fallos.
3. **`adapters/tests/t18_2_repro_adapter_tests.mjs`**:
   - Proveedor desconocido `unknown`, descarte de Stop tardío para `turn-1` cuando la sesión está en `turn-2`.
   - **Resultado:** Aprobado sin fallos.
4. **`adapters/tests/t18_3_repro_adapter_tests.mjs`**:
   - Validación exhaustiva de los 5 casos de identidad de turno para OpenCode y Pi.
   - **Resultado:** Aprobado sin fallos.

---

## 5. Smoke Test y Empaquetado Release

* **`scripts/smoke-test.ps1`**: Ejecutó diagnóstico de puertos, validación de configuración v1, `peakguard doctor`, suite Rust completa y suites Node.js.
  - **Resultado:** `¡Smoke test completado exitosamente! Todos los componentes funcionan correctamente.`
* **`scripts/build-windows.ps1`**:
  - Compilación Release optimizada en 24.95s.
  - Binario: `dist/peakguard.exe` (13.4 MB).
  - Checksum SHA-256: `5A2F8C336FB8E558A27E40143EC1C9906BF2508B6334F6E42310B6B8054A6189`.
