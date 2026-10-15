# PeakGuard — Evidencias Técnicas de Pruebas: Fase T17

**Fecha:** 15 de octubre de 2026  
**Entorno de ejecución:** Windows 11 Build 10.0.26200.0 x64  
**Compilador:** Rust 1.99.0 (`x86_64-pc-windows-gnu`)  
**Runtime:** Node.js v24.13.1  
**Total de pruebas ejecutadas:** 41 pruebas automatizadas en Rust + 4 suites de prueba en Node.js + Smoke Test completo.

---

## 1. Evidencia de Suite Completa de Pruebas Rust (`cargo test`)

```text
running 8 tests
test config::tests::test_conflicting_ports ... ok
test config::tests::test_invalid_mode ... ok
test config::tests::test_missing_referenced_schedule ... ok
test config::tests::test_valid_default_config ... ok
test notifications::tests::test_deduplication_exact_sequence ... ok
test registry::tests::test_session_registry_lifecycle ... ok
test registry::tests::test_session_registry_filter_by_schedule ... ok
test registry::tests::test_session_registry_prune_stale ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\main.rs
test config::tests::test_conflicting_ports ... ok
test config::tests::test_invalid_mode ... ok
test config::tests::test_missing_referenced_schedule ... ok
test config::tests::test_valid_default_config ... ok
test notifications::tests::test_deduplication_exact_sequence ... ok
test registry::tests::test_session_registry_filter_by_schedule ... ok
test registry::tests::test_session_registry_lifecycle ... ok
test registry::tests::test_session_registry_prune_stale ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests\gateway_e2e_tests.rs
test test_gateway_allow_and_block_e2e ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests\gateway_streaming_tests.rs
test test_streaming_sse_and_concurrency_20_streams ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

     Running tests\mcp_server_tests.rs
test test_mcp_coverage_tool ... ok
test test_mcp_initialize_and_ping ... ok
test test_mcp_invalid_json_and_unknown_methods ... ok
test test_mcp_models_tool ... ok
test test_mcp_next_peak_tool ... ok
test test_mcp_security_guard_disallows_mutation ... ok
test test_mcp_status_tool_transitions ... ok
test test_mcp_tools_list ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests\policy_mode_tests.rs
test test_notify_only_mode ... ok
test test_off_peak_allows_all ... ok
test test_restricted_stop_mode_idempotency ... ok
test test_runtime_mode_switch ... ok
test test_stop_deduplication_across_minutes_in_same_window ... ok
test test_strict_block_mode ... ok
test test_third_party_models_never_blocked ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\real_daemon_e2e_tests.rs
test test_real_daemon_binary_lifecycle_and_ports ... ok
test test_real_daemon_simulated_time_peak_transition ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.69s

     Running tests\schedule_boundary_tests.rs
test test_all_boundary_fixtures ... ok
test test_clock_jump_detector ... ok
test test_injectable_test_clock ... ok
test test_model_registry_resolution_and_coverage ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\stream_cancellation_tests.rs
test test_active_stream_cancellation_on_peak_transition ... ok
test test_active_stream_uninterrupted_in_notify_only_mode ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
```

---

## 2. Evidencia de Suite de Verificación de Adaptadores en Node.js

Comando: `node adapters/tests/adapter-mock-runner.test.mjs`

```text
[TEST] Mock Control API escuchando en el puerto 61112
--- 1. Probando fetchStatus() en Widget TUI ---
  ✓ Widget TUI sincronizó estado correctamente.
--- 2. Probando registro de sesiones activas (T17.2) ---
  ✓ Sesión registrada con token Bearer verificado.
--- 3. Probando Stop nativo exitoso en OpenCode (T17.2, T17.6) ---
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-test-1'. Abortando turno activo...
[PeakGuard:OpenCode] Turno abortado exitosamente mediante client.session.abort({ id: 'session-test-1' }).
  ✓ client.session.abort() ejecutado y confirmado con Stop ACK exitoso.
--- 4. Probando Stop con fallo: NUNCA reportar falso positivo (T17.1, T17.6) ---
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-failing-1'. Abortando turno activo...
[PeakGuard:OpenCode] Error invocando client.session.abort: Error: Simulated editor crash during abort
  ✓ Fallo de abort reporta estrictamente success=false sin falsos positivos.

=========================================
TODAS LAS PRUEBAS DE MOCK RUNNER PASARON!
=========================================
```

---

## 3. Evidencia del Smoke Test de Verificación Integral (`scripts/smoke-test.ps1`)

Comando: `powershell -ExecutionPolicy Bypass -File scripts/smoke-test.ps1`

```text
==> Ejecutando Smoke Test contra: C:\Users\meliodaslosven\Desktop\Projects\deepseek\Deepseek-Peak-Guard\core\target\x86_64-pc-windows-gnu\release\peakguard.exe

1. Probando comando 'version'...
PeakGuard Core v0.1.0
Target: x86_64-pc-windows-gnu
Plataforma: Windows 10/11 x64
Especificación: 1.0 (2026-10-14)
Estado: Núcleo autónomo local compilado

2. Probando comando 'config validate'...
Validando archivo de configuración: C:\Users\meliodaslosven\Desktop\Projects\deepseek\Deepseek-Peak-Guard\config\peakguard.example.toml
✅ Configuración válida (versión 1)
  - Modo: restricted_stop
  - Zona de presentación: America/Lima
  - Gateway: 127.0.0.1:8765
  - Control API: 127.0.0.1:8766
  - Proveedores configurados: 2
    * deepseek_direct (habilitado: true, horario: deepseek_direct_verified)
    * opencode_go (habilitado: false, horario: opencode_go_verified)
  - Horarios definidos: 2
    * deepseek_direct_verified [UTC]: ["01:00-04:00", "06:00-10:00"]
    * opencode_go_verified [UTC]: ["01:00-04:00", "06:00-10:00"]

3. Probando comando 'doctor'...
=== PeakGuard Doctor: Diagnóstico del Sistema ===

[1/5] Sistema Operativo:
  - OS: Windows x64 compatible
  - Privilegios: Usuario ordinario (OK, sin elevación)
[2/5] Configuración:
  - Archivo 'C:\Users\meliodaslosven\Desktop\Projects\deepseek\Deepseek-Peak-Guard\config\peakguard.example.toml': VÁLIDO (v1)
[3/5] Disponibilidad de Puertos Locales:
  - Puerto 8765 (Gateway Data Plane): DISPONIBLE
  - Puerto 8766 (Control API Plane): DISPONIBLE
[4/5] Detección de Clientes:
  - OpenCode: INSTALADO (C:\Users\meliodaslosven\.config\opencode)
  - Pi Coding Agent: INSTALADO (C:\Users\meliodaslosven\.pi\agent)
  - Hermes Agent: INSTALADO (C:\Users\meliodaslosven\AppData\Local\hermes)
[5/5] Diagnóstico de Cobertura:
  - Proveedor 'deepseek_direct' [PROTEGIDO / VERIFICADO]: upstream=Some("https://api.deepseek.com"), schedule='deepseek_direct_verified'
  - Proveedor 'opencode_go' [NO VERIFICADO / UNVERIFIED]: Inferencia gestionada por OpenCode; requiere custom baseURL para pasar por Gateway

Diagnóstico finalizado. Todo listo para ejecución con 'peakguard run'.

4. Ejecutando suite completa de pruebas unitarias y de integración...
...
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s

==> ¡Smoke test completado exitosamente! Todos los componentes funcionan correctamente.
```

---

## 4. Matriz de Cobertura de Criterios de Aceptación T17

| Requisito / Criterio | Prueba Específica | Resultado |
|---|---|---|
| **Auth 401 en /config/mode sin token** | `real_daemon_e2e_tests::test_real_daemon_binary_lifecycle_and_ports` | ✅ PASADO (HTTP 401 recibido) |
| **Auth 401 en /adapters/stop-ack sin token** | `real_daemon_e2e_tests::test_real_daemon_binary_lifecycle_and_ports` | ✅ PASADO (HTTP 401 recibido) |
| **Rechazo 422 en acuse sin campo `success`** | `real_daemon_e2e_tests::test_real_daemon_binary_lifecycle_and_ports` | ✅ PASADO (HTTP 422 recibido) |
| **Registro de sesiones activas** | `registry::tests::test_session_registry_lifecycle` y E2E | ✅ PASADO (Registro y Heartbeat OK) |
| **Stop nativo no mata proceso ni app** | `adapters/tests/adapter-mock-runner.test.mjs` | ✅ PASADO (`session.abort()` verificado) |
| **Fallo de stop reporta error real** | `adapters/tests/adapter-mock-runner.test.mjs` | ✅ PASADO (`success: false`, sin falsos positivos) |
| **Cobertura honesta (DISCONNECTED inicial)**| `adapters/opencode/peakguard-tui-sidebar.js` | ✅ PASADO (`DISCONNECTED` en desconexión) |
| **Cancelación autónoma por reloj** | `stream_cancellation_tests::test_active_stream_cancellation_on_peak_transition` | ✅ PASADO (Cutoff sin chunks) |
| **Invarianza de StopKey por ventana** | `policy_mode_tests::test_stop_deduplication_across_minutes_in_same_window` | ✅ PASADO (1 stop por ventana) |
| **Bloqueo de modelo protegido en pico** | `real_daemon_e2e_tests::test_real_daemon_simulated_time_peak_transition` | ✅ PASADO (HTTP 403 en `deepseek-chat`) |
| **Modelo valle sin bloqueo** | `real_daemon_e2e_tests::test_real_daemon_simulated_time_peak_transition` | ✅ PASADO (HTTP 200 en `deepseek-v4.1-flash`) |
| **Apagado ordenado limpio** | `real_daemon_e2e_tests::test_real_daemon_binary_lifecycle_and_ports` | ✅ PASADO (`child.wait()` exit status 0) |
