# PeakGuard — T18: Evidencia de Pruebas y Validación Técnica

**Fecha de ejecución:** 15 de octubre de 2026  
**Entorno:** Windows 11 Build 10.0.26200.0 (Win32NT) x64  
**Compilador:** Rust 1.99.0 (`cargo 1.99.0`) | **Node.js:** v24.13.1  

---

## 1. Evidencia de Pruebas de Adaptadores Node.js

### 1.1. `adapters/tests/adapter-mock-runner.test.mjs`

```text
[TEST] Mock Control API escuchando en el puerto 60869
--- 1. Probando fetchStatus() en Widget TUI ---
  ✓ Widget TUI sincronizó estado correctamente.
--- 2. Probando registro de sesiones activas (T17.2, T18.4) ---
  ✓ Sesión registrada con token Bearer verificado.
--- 3. Probando Stop nativo exitoso en OpenCode (T17.2, T17.6) ---
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-test-1'. Abortando turno activo...
[PeakGuard:OpenCode] Turno abortado exitosamente mediante client.session.abort({ id: 'session-test-1' }).
  ✓ client.session.abort() ejecutado y confirmado con Stop ACK exitoso.
--- 4. Probando Stop con fallo: NUNCA reportar falso positivo (T17.1, T17.6) ---
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-failing-1'. Abortando turno activo...
[PeakGuard:OpenCode] Error invocando client.session.abort: Error: Simulated editor crash during abort
    at Object.abort (file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/tests/adapter-mock-runner.test.mjs:189:17)
    at IncomingMessage.<anonymous> (file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/opencode/peakguard-opencode-server.js:232:40)
  ✓ Fallo de abort reporta estrictamente success=false sin falsos positivos.
--- 5. Probando aislamiento de sesión en OpenCode (ignora sesiones ajenas) ---
[PeakGuard:OpenCode] Sesión 'session-foreign-unknown-999' no pertenece a este cliente OpenCode. Ignorando orden.
  ✓ OpenCode ignora Stop de sesiones ajenas no registradas en su contexto.
--- 6. Probando adaptador de Pi: aislamiento de sesión, abort nativo y settled ---
  Enviando stop_requested para 'opencode-foreign-session-888'...
  ✓ Pi ignoró exitosamente el stop para sesión ajena.
  Enviando stop_requested para la sesión propia 'pi-isolated-session'...
[PeakGuard:Pi] Orden de Stop recibida para 'pi-isolated-session'. Abortando turno activo...
[PeakGuard:Pi] activeCtx.abort() ejecutado con éxito.
  ✓ Pi ejecutó abort() nativo y envió Stop ACK confirmado.
  ✓ Pi envió is_active: false en agent_settled.

=========================================
TODAS LAS PRUEBAS DE MOCK RUNNER PASARON!
=========================================
```

### 1.2. `adapters/tests/repro-pi-foreign-session-abort.test.mjs`

```text
[REPRO F] Emitiendo orden de Stop para sesión ajena 'opencode-foreign-session-999'...
[REPRO F] ¿activeCtx.abort() fue llamado?: false
✓ Corrección F verificada: Pi ignora órdenes de Stop de sesiones ajenas.
```

---

## 2. Evidencia de Pruebas Unitarias y de Integración en Rust (`cargo test --tests`)

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.64s
     Running unittests src\lib.rs (target\x86_64-pc-windows-gnu\debug\deps\peakguard_core-fddc02f1f3a0ef2b.exe)

running 8 tests
test registry::tests::test_session_registry_lifecycle ... ok
test registry::tests::test_session_registry_filter_by_schedule ... ok
test config::tests::test_invalid_mode ... ok
test config::tests::test_conflicting_ports ... ok
test config::tests::test_missing_referenced_schedule ... ok
test registry::tests::test_session_registry_prune_stale ... ok
test config::tests::test_valid_default_config ... ok
test notifications::tests::test_deduplication_exact_sequence ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\main.rs (target\x86_64-pc-windows-gnu\debug\deps\peakguard-b271446d6c801ebb.exe)

running 8 tests
test registry::tests::test_session_registry_lifecycle ... ok
test registry::tests::test_session_registry_prune_stale ... ok
test registry::tests::test_session_registry_filter_by_schedule ... ok
test config::tests::test_invalid_mode ... ok
test config::tests::test_missing_referenced_schedule ... ok
test config::tests::test_conflicting_ports ... ok
test config::tests::test_valid_default_config ... ok
test notifications::tests::test_deduplication_exact_sequence ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\gateway_e2e_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\gateway_e2e_tests-d6e9ddc9172e4762.exe)

running 1 test
test test_gateway_allow_and_block_e2e ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

     Running tests\gateway_streaming_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\gateway_streaming_tests-c7d72828da489d35.exe)

running 1 test
test test_streaming_sse_and_concurrency_20_streams ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s

     Running tests\mcp_server_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\mcp_server_tests-129bab85bdc67f94.exe)

running 8 tests
test test_mcp_security_guard_disallows_mutation ... ok
test test_mcp_next_peak_tool ... ok
test test_mcp_initialize_and_ping ... ok
test test_mcp_models_tool ... ok
test test_mcp_status_tool_transitions ... ok
test test_mcp_tools_list ... ok
test test_mcp_coverage_tool ... ok
test test_mcp_invalid_json_and_unknown_methods ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\policy_mode_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\policy_mode_tests-1b5e9abc871aca58.exe)

running 7 tests
test test_third_party_models_never_blocked ... ok
test test_off_peak_allows_all ... ok
test test_notify_only_mode ... ok
test test_strict_block_mode ... ok
test test_stop_deduplication_across_minutes_in_same_window ... ok
test test_runtime_mode_switch ... ok
test test_restricted_stop_mode_idempotency ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\real_daemon_e2e_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\real_daemon_e2e_tests-b9aa81bfee591215.exe)

running 2 tests
test test_real_daemon_simulated_time_peak_transition ... ok
test test_real_daemon_binary_lifecycle_and_ports ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s

     Running tests\schedule_boundary_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\schedule_boundary_tests-5b3cf30b8891ce71.exe)

running 4 tests
test test_clock_jump_detector ... ok
test test_all_boundary_fixtures ... ok
test test_model_registry_resolution_and_coverage ... ok
test test_injectable_test_clock ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests\stream_cancellation_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\stream_cancellation_tests-5d104436098b4147.exe)

running 2 tests
test test_active_stream_cancellation_on_peak_transition ... ok
test test_active_stream_uninterrupted_in_notify_only_mode ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s

     Running tests\t18_security_and_lifecycle_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\t18_security_and_lifecycle_tests-37073155d1b6face.exe)

running 6 tests
test test_lifecycle_active_reasoning_turn_preserved_across_silence ... ok
test test_security_csprng_token_generation_and_uniqueness ... ok
test test_security_sse_query_token_rejected_and_bearer_header_accepted ... ok
test test_opencode_go_reports_unverified_without_custom_gateway_traffic ... ok
test test_security_empty_expected_token_rejected_fail_closed ... ok
test test_verifiable_coverage_transitions_from_partial_to_protected ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

---

## 3. Evidencia del Smoke Test de Binario (`scripts/smoke-test.ps1`)

```powershell
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
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.06s
    ... (47 tests passed) ...

==> ¡Smoke test completado exitosamente! Todos los componentes funcionan correctamente.
```

---

## 4. Evidencia de Compilación Release (`cargo build --release`)

```text
   Compiling peakguard-core v0.1.0 (C:\Users\meliodaslosven\Desktop\Projects\deepseek\Deepseek-Peak-Guard\core)
    Finished `release` profile [optimized] target(s) in 1m 08s
```
Binary generated at: `core/target/x86_64-pc-windows-gnu/release/peakguard.exe`
