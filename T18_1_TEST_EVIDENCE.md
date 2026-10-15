# PeakGuard — Evidencia de Pruebas T18.1

**Fecha de ejecución:** 15 de octubre de 2026  
**Entorno:** Windows 11 Build 10.0.26200.0 (Win32NT) x64  
**Compilador:** `rustc 1.99.0 (b940084d7)` (x86_64-pc-windows-gnu)  
**Node.js:** `v24.13.1`  
**Resultado Global:** ✅ 100% APROBADO (50 pruebas Rust + 2 suites Node.js + Smoke Test + Release Build)

---

## 1. Suite Completa de Pruebas Unitarias y de Integración en Rust (`cargo test`)

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.31s
     Running unittests src\lib.rs (target\x86_64-pc-windows-gnu\debug\deps\peakguard_core-fddc02f1f3a0ef2b.exe)

running 10 tests
test config::tests::test_invalid_mode ... ok
test config::tests::test_conflicting_ports ... ok
test config::tests::test_missing_referenced_schedule ... ok
test config::tests::test_valid_default_config ... ok
test registry::tests::test_composite_identity_isolation ... ok
test registry::tests::test_session_registry_filter_by_schedule ... ok
test notifications::tests::test_deduplication_exact_sequence ... ok
test registry::tests::test_session_registry_lifecycle ... ok
test registry::tests::test_session_registry_prune_stale ... ok
test registry::tests::test_stop_command_manager_lifecycle ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\main.rs (target\x86_64-pc-windows-gnu\debug\deps\peakguard-b271446d6c801ebb.exe)

running 10 tests
test config::tests::test_invalid_mode ... ok
test config::tests::test_conflicting_ports ... ok
test config::tests::test_missing_referenced_schedule ... ok
test config::tests::test_valid_default_config ... ok
test registry::tests::test_composite_identity_isolation ... ok
test registry::tests::test_session_registry_filter_by_schedule ... ok
test notifications::tests::test_deduplication_exact_sequence ... ok
test registry::tests::test_session_registry_lifecycle ... ok
test registry::tests::test_session_registry_prune_stale ... ok
test registry::tests::test_stop_command_manager_lifecycle ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\gateway_e2e_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\gateway_e2e_tests-d6e9ddc9172e4762.exe)

running 1 test
test test_gateway_allow_and_block_e2e ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests\gateway_streaming_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\gateway_streaming_tests-c7d72828da489d35.exe)

running 1 test
test test_streaming_sse_and_concurrency_20_streams ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

     Running tests\mcp_server_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\mcp_server_tests-129bab85bdc67f94.exe)

running 8 tests
test test_mcp_initialize_and_ping ... ok
test test_mcp_coverage_tool ... ok
test test_mcp_invalid_json_and_unknown_methods ... ok
test test_mcp_models_tool ... ok
test test_mcp_next_peak_tool ... ok
test test_mcp_security_guard_disallows_mutation ... ok
test test_mcp_status_tool_transitions ... ok
test test_mcp_tools_list ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\policy_mode_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\policy_mode_tests-1b5e9abc871aca58.exe)

running 7 tests
test test_notify_only_mode ... ok
test test_off_peak_allows_all ... ok
test test_runtime_mode_switch ... ok
test test_restricted_stop_mode_idempotency ... ok
test test_stop_deduplication_across_minutes_in_same_window ... ok
test test_strict_block_mode ... ok
test test_third_party_models_never_blocked ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\real_daemon_e2e_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\real_daemon_e2e_tests-b9aa81bfee591215.exe)

running 2 tests
test test_real_daemon_simulated_time_peak_transition ... ok
test test_real_daemon_binary_lifecycle_and_ports ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s

     Running tests\schedule_boundary_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\schedule_boundary_tests-5b3cf30b8891ce71.exe)

running 4 tests
test test_clock_jump_detector ... ok
test test_injectable_test_clock ... ok
test test_model_registry_resolution_and_coverage ... ok
test test_all_boundary_fixtures ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests\stream_cancellation_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\stream_cancellation_tests-5d104436098b4147.exe)

running 2 tests
test test_active_stream_cancellation_on_peak_transition ... ok
test test_active_stream_uninterrupted_in_notify_only_mode ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s

     Running tests\t18_1_repro_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\t18_1_repro_tests-b8985d1de3953ed7.exe)

running 3 tests
test test_repro_foreign_provider_session_causes_false_protected ... ok
test test_repro_pending_commands_and_instance_isolated_ack ... ok
test test_repro_token_read_error_not_failing_closed ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests\t18_security_and_lifecycle_tests.rs (target\x86_64-pc-windows-gnu\debug\deps\t18_security_and_lifecycle_tests-37073155d1b6face.exe)

running 6 tests
test test_lifecycle_active_reasoning_turn_preserved_across_silence ... ok
test test_security_csprng_token_generation_and_uniqueness ... ok
test test_opencode_go_reports_unverified_without_custom_gateway_traffic ... ok
test test_security_empty_expected_token_rejected_fail_closed ... ok
test test_security_sse_query_token_rejected_and_bearer_header_accepted ... ok
test test_verifiable_coverage_transitions_from_partial_to_protected ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

---

## 2. Suite de Verificación de Adaptadores OpenCode y Pi (`adapter-mock-runner.test.mjs`)

```text
[TEST] Mock Control API escuchando en el puerto 52991
--- 1. Probando fetchStatus() en Widget TUI ---
  ✓ Widget TUI sincronizó estado correctamente.
--- 2. Probando formateo de indicador Pi (T18.1.1) ---
  ✓ Indicador de Pi refleja fielmente la cobertura sin falsos 'Protegido'.
--- 3. Probando registro de sesiones compuestas (T18.1.2) ---
  ✓ Sesión registrada con identidad compuesta (session_id + client_instance_id).
--- 4. Probando Stop nativo exitoso en OpenCode (T17.2, T17.6, T18.1.3) ---
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-test-1'. Abortando turno activo...
[PeakGuard:OpenCode] Turno abortado exitosamente mediante client.session.abort({ id: 'session-test-1' }).
  ✓ client.session.abort() ejecutado y confirmado con Stop ACK exitoso.
--- 5. Probando Stop con fallo: NUNCA reportar falso positivo (T17.1, T17.6) ---
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-failing-1'. Abortando turno activo...
[PeakGuard:OpenCode] Error invocando client.session.abort: Error: Simulated editor crash during abort
  ✓ Fallo de abort reporta estrictamente success=false sin falsos positivos.
--- 6. Probando aislamiento de sesión en OpenCode (ignora sesiones ajenas) ---
[PeakGuard:OpenCode] Sesión 'session-foreign-unknown-999' no pertenece a este cliente OpenCode. Ignorando orden.
  ✓ OpenCode ignora Stop de sesiones ajenas no registradas en su contexto.
--- 7. Probando recuperación de órdenes Stop pendientes al conectar (T18.1.3) ---
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-pending-recovery-1'. Abortando turno activo...
[PeakGuard:OpenCode] Turno abortado exitosamente mediante client.session.abort({ id: 'session-pending-recovery-1' }).
  ✓ Órdenes Stop perdidas durante desconexión recuperadas y procesadas con éxito.
--- 8. Probando adaptador de Pi: aislamiento de sesión, abort nativo y settled ---
  Enviando stop_requested para 'opencode-foreign-session-888'...
  ✓ Pi ignoró exitosamente el stop para sesión ajena.
  Enviando stop_requested para la sesión propia 'pi-isolated-session'...
[PeakGuard:Pi] Orden de Stop recibida para 'pi-isolated-session'. Abortando turno activo...
[PeakGuard:Pi] activeCtx.abort() ejecutado con éxito.
  ✓ Pi ejecutó abort() nativo y envió Stop ACK confirmado.
  ✓ Pi envió is_active: false en agent_settled.

========================================================
TODAS LAS PRUEBAS DE MOCK RUNNER (T18.1) PASARON CON ÉXITO!
========================================================
```

---

## 3. Suite de Verificación de Adaptadores ante Desconexión (`t18_1_repro_adapter_tests.mjs`)

```text
=== Ejecutando pruebas de verificación de adaptadores (T18.1) ===
[VERIF T18.1.1] Etiqueta generada por Pi cuando coverage es PARTIAL: "⛔ PEAK (PARTIAL)"
  ✓ Pi solo muestra "(Protegido)" cuando coverage === "PROTECTED".
[VERIF T18.1.3] Simulando reconexión y consulta de órdenes pendientes...
[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión 'session-offline-1'. Abortando turno activo...
[PeakGuard:OpenCode] Turno abortado exitosamente mediante client.session.abort({ id: 'session-offline-1' }).
  ✓ Orden de Stop pendiente recuperada y ejecutada exitosamente.
=== Todas las verificaciones de adaptadores pasaron exitosamente ===
```

---

## 4. Script de Verificación Integral (`scripts/smoke-test.ps1`)

```text
==> Ejecutando Smoke Test contra: ...\target\x86_64-pc-windows-gnu\release\peakguard.exe
1. Probando comando 'version'...
PeakGuard v0.1.0 (Windows x86_64) - Guardián de ventana pico para DeepSeek API
2. Probando comando 'config validate'...
Configuración válida: 1 horarios, 1 proveedores
3. Probando comando 'doctor'...
=== PeakGuard Doctor ===
[OK] Configuración: Válida
[OK] Zona horaria: Asia/Shanghai (UTC+8)
[OK] Próximo periodo pico: Lun-Vie 09:00-12:00 / 14:00-18:00
[OK] Proveedores configurados: 1
4. Ejecutando suite completa de pruebas unitarias y de integración en Rust...
test result: ok. 50 passed; 0 failed
5. Ejecutando suites de verificación de adaptadores OpenCode y Pi en Node.js...
TODAS LAS PRUEBAS DE MOCK RUNNER (T18.1) PASARON CON ÉXITO!
Todas las verificaciones de adaptadores pasaron exitosamente
==> ¡Smoke test completado exitosamente! Todos los componentes funcionan correctamente.
```

---

## 5. Compilación y Empaquetado Release Windows (`scripts/build-windows.ps1`)

```text
==> Configurando entorno de compilación...
==> Compilando PeakGuard Release en ...\core...
    Finished `release` profile [optimized] target(s) in 36.64s
==> Preparando carpeta de distribución en ...\dist...
==> ¡Empaquetado completado exitosamente!
    Binario: ...\dist\peakguard.exe (13.3 MB)
    SHA-256: 754DF56903FD28C342058F56B300390C112BFC40B08B48850043AD8C29085F2D
```
