# Evidencia de Pruebas Técnicas — Fase T19

## 1. Entorno de Ejecución

* **Sistema Operativo:** Windows 10/11 x64 (Build 19045+)
* **Rust Toolchain:** rustc 1.85.0+ (x86_64-pc-windows-gnu)
* **Node.js Runtime:** v24.13.1
* **Binario Evaluado:** `dist/peakguard.exe` (13.4 MB, SHA-256: `5A2F8C336FB8E558A27E40143EC1C9906BF2508B6334F6E42310B6B8054A6189`)
* **Detección de Clientes Locales (`peakguard doctor`):**
  - OpenCode: INSTALADO (`C:\Users\meliodaslosven\.config\opencode`)
  - Pi Coding Agent: INSTALADO (`C:\Users\meliodaslosven\.pi\agent`)
  - Hermes Agent: INSTALADO (`C:\Users\meliodaslosven\AppData\Local\hermes`)

---

## 2. Prueba Principal de Integración E2E: `adapters/tests/t19_e2e_integration.test.mjs`

### Comando Ejecutado
```powershell
node adapters/tests/t19_e2e_integration.test.mjs
```

### Salida de Consola y Verificación Empírica
```
===================================================================
PeakGuard T19: Pruebas de Integración E2E con Clientes y Upstream
===================================================================
[T19] Mock Upstream iniciado en http://127.0.0.1:50781
[T19] Iniciando demonio real: C:\Users\meliodaslosven\Desktop\Projects\deepseek\Deepseek-Peak-Guard\dist\peakguard.exe
[T19] Reloj virtual fijado a 2026-10-15T02:00:00Z (Horario Peak de DeepSeek)
  ✓ Gateway (puerto 50791) y Control API (puerto 50792) listos y saludables.

--- [T19.1] Verificación de Hermes Agent ---
  ✓ Petición de Hermes a deepseek-chat bloqueada correctamente con HTTP 403 (sin cerrar el proceso Hermes).
  ✓ Petición de Hermes a modelo off-peak (deepseek-v4.1-flash) respondió 200 OK desde el upstream.

--- [T19.2] Verificación del Adaptador OpenCode ---
  ✓ Sesión de OpenCode registrada exitosamente en Control API.
  ✓ Heartbeat de OpenCode aceptado por la Control API.
[PeakGuard:OpenCode] unverifiable_turn_identity: event.turn_id ausente en la orden de Stop. Evitando abort ambiguo y aplicando fallback de gateway.
  ✓ Stop ambiguo rechazado de forma segura sin abortar sesión.
[PeakGuard:OpenCode] Recibida orden de Stop nativo válida para sesión 'sess-oc-t19-real' (turno 'turn-oc-t19-alpha'). Abortando turno activo...
[PeakGuard:OpenCode] Turno 'turn-oc-t19-alpha' abortado exitosamente mediante client.session.abort({ id: 'sess-oc-t19-real' }).
  ✓ Stop nativo ejecutado exitosamente mediante client.session.abort().
  ✓ Sesión de OpenCode desregistrada limpiamente.

--- [T19.3] Verificación de la Extensión Pi Coding Agent ---
  ✓ Widget UI de Pi muestra estado fiel: '🛡️ OFF-PEAK (--)'
  ✓ Pi Extension registró turno activo en Control API.
[PeakGuard:Pi] Orden de Stop recibida para 'sess-pi-t19-real'. Abortando turno activo...
[PeakGuard:Pi] activeCtx.abort() ejecutado con éxito.
  ✓ Stop nativo en Pi ejecutado con éxito mediante ctx.abort() (sin cerrar proceso).

--- [T19.4] Conmutación Dinámica de Modos y Streaming SSE ---
  ✓ Streaming SSE intermediado transparentemente en modo notify_only.
  ✓ Modo strict_block bloquea estrictamente las peticiones en horario pico.

--- [T19.5] Apagado Ordenado del Demonio ---
[T19] Demonio PeakGuard finalizó con código: 0
  ✓ Demonio PeakGuard apagado ordenadamente con código de salida 0.

===================================================================
¡TODAS LAS PRUEBAS DE INTEGRACIÓN REAL T19 PASARON CON ÉXITO!
===================================================================
```

---

## 3. Pruebas de Integración en Rust: `core/tests/t19_integration_tests.rs`

### Comando Ejecutado
```powershell
cargo test --test t19_integration_tests -- --nocapture
```

### Salida de Consola
```
running 3 tests
test test_t19_session_registry_stop_ack_and_isolation ... ok
test test_t19_streaming_cancellation_and_no_auto_resume ... ok
test test_t19_three_modes_virtual_clock_against_mock_upstream ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
```

---

## 4. Regresión Completa de Rust (63 Tests en Core)

### Comando Ejecutado
```powershell
cargo test --tests -- --test-threads=2
```

### Resumen de Resultados
* `unittests src\lib.rs`: 10 passed; 0 failed
* `unittests src\main.rs`: 10 passed; 0 failed
* `gateway_e2e_tests.rs`: 1 passed; 0 failed
* `gateway_streaming_tests.rs`: 1 passed; 0 failed
* `mcp_server_tests.rs`: 8 passed; 0 failed
* `policy_mode_tests.rs`: 7 passed; 0 failed
* `real_daemon_e2e_tests.rs`: 2 passed; 0 failed
* `schedule_boundary_tests.rs`: 4 passed; 0 failed
* `stream_cancellation_tests.rs`: 2 passed; 0 failed
* `t18_1_repro_tests.rs`: 3 passed; 0 failed
* `t18_2_repro_tests.rs`: 3 passed; 0 failed
* `t18_3_repro_tests.rs`: 3 passed; 0 failed
* `t18_security_and_lifecycle_tests.rs`: 6 passed; 0 failed
* `t19_integration_tests.rs`: 3 passed; 0 failed
* **Total:** **63 aprobadas; 0 fallidas; 0 ignoradas.**

---

## 5. Batería de Suites de Adaptadores en Node.js

| Suite | Comando | Estado |
|---|---|---|
| Mock Runner T18.1 | `node adapters/tests/adapter-mock-runner.test.mjs` | **100% OK** |
| Verificación T18.1 | `node adapters/tests/t18_1_repro_adapter_tests.mjs` | **100% OK** |
| Reproducción T18.2 | `node adapters/tests/t18_2_repro_adapter_tests.mjs` | **100% OK** |
| Reproducción T18.3 | `node adapters/tests/t18_3_repro_adapter_tests.mjs` | **100% OK** |
| Integración E2E T19 | `node adapters/tests/t19_e2e_integration.test.mjs` | **100% OK** |

---

## 6. Verificación de Smoke Test (`scripts/smoke-test.ps1`)

### Salida
```
==> 1. Compilando binario de prueba...
==> 2. Verificando diagnóstico del sistema ('peakguard doctor')...
[1/5] Sistema Operativo: Windows x64 compatible
[2/5] Configuración: VÁLIDA
[3/5] Disponibilidad de Puertos Locales: 8765 y 8766 DISPONIBLES
[4/5] Detección de Clientes: OpenCode, Pi, Hermes INSTALADOS
[5/5] Diagnóstico de Cobertura: deepseek_direct PROTEGIDO, opencode_go UNVERIFIED
==> 3. Verificando servidor MCP ('peakguard mcp')...
==> 4. Ejecutando batería completa de pruebas unitarias y de integración...
==> 5. Ejecutando suites de verificación de adaptadores OpenCode y Pi en Node.js...
==> ¡Smoke test completado exitosamente! Todos los componentes funcionan correctamente.
```
