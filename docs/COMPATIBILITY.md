# Matriz de Compatibilidad y Capacidades de Clientes y Proveedores (T18.2)

**Fecha de auditoría y estabilización:** 15 de octubre de 2026  
**Entorno de ejecución verificado:** Windows 11 Build 10.0.26200.0 (Win32NT) x64  
**Herramientas base:** Rust 1.99.0 (`cargo 1.99.0`), Node.js v24.13.1, Python 3.11.16

---

## 1. Inventario de Entorno y Clientes Instalados

| Aplicación / Herramienta | Versión Instalada | Método de Instalación / Ruta | Soporte Gateway HTTP | Soporte Abort / Stop Nativo | Estado de Verificación |
|---|---|---|---|---|---|
| **OpenCode** | `1.18.35` | `~/.gentle-ai/bin/opencode.ps1` (`npm/opencode`) | ✅ Sí (vía `@ai-sdk/openai-compatible` con `baseURL`) | ✅ Sí (`session.abort({ path })` / hooks de plugin) | `TESTED_WITH_MOCKS` (Validado en suite Node.js) |
| **Pi Coding Agent** | `0.87.1` | `@earendil-works/pi-coding-agent` (`npm/pi.ps1`) | ✅ Sí (`~/.pi/agent/models.json` con `baseUrl`) | ✅ Sí (`ctx.abort()`, no `ctx.shutdown()`) | `TESTED_WITH_MOCKS` (Validado en suite Node.js) |
| **Hermes Agent** | `0.21.4 (2026.9.21)` | Git (`AppData/Local/hermes/hermes-agent`) | ✅ Sí (`config.yaml` con `model.base_url`) | ⚠️ Condicionado (`/stop` detiene subprocesos) | `PARTIAL` |
| **Rust Toolchain** | `1.99.0 (b940084d7)` | `rustup` x86_64-pc-windows-msvc | N/A (Entorno de compilación) | N/A | `VERIFIED` |

---

## 2. Matriz de Proveedores y Modelos Facturables

| Proveedor | Endpoint Base Real | Modelos / Aliases Típicos | Horarios Peak (UTC) | Excepción Festivos Chinos | Soporte Routing Gateway |
|---|---|---|---|---|---|
| **DeepSeek Direct API** | `https://api.deepseek.com/v1` | `deepseek-chat`, `deepseek-reasoner` | Lun–Vie 01:00–04:00 y 06:00–10:00 UTC | ✅ Sí (oficial en pricing) | `VERIFIED` (Probado e2e contra binario) |
| **OpenCode Go** | `https://opencode.ai/zen/go/v1` | `opencode-go/deepseek-v4-flash`, `deepseek-v4.1-flash` | Lun–Vie 01:00–04:00 y 06:00–10:00 UTC | ❌ No especificada (tratar como NO) | `UNVERIFIED` (Inferencia nativa gestionada; requiere custom provider con baseURL local para ser `PROTECTED`) |
| **TokenHarbor / Resellers** | `https://tokenharbor.ai/v1` | `deepseek-v4.1-flash` | Dependiente del reseller (no inferir) | ❌ No especificada | `UNVERIFIED` (perfil propio requerido) |

---

## 3. Detalle de Compatibilidad por Cliente

### 3.1. OpenCode (`1.18.35`)
- **Arquitectura de Plugins dividida:**
  - **Servidor / Headless:** `adapters/opencode/peakguard-opencode-server.js` para `opencode.jsonc` (`"plugin": [...]`). Registra sesiones en Control API con Bearer Token, escucha eventos SSE con cabecera `Authorization` exclusiva y ejecuta `client.session.abort({ path: { id } })`.
  - **TUI / Interfaz:** `adapters/opencode/peakguard-tui-sidebar.js` para `tui.jsonc` (`"plugin": [...]`). Renderiza widget lateral y status line con cuenta regresiva. Estado inicial estricto: `DISCONNECTED`.
  - **Compatibilidad Unificada:** `adapters/opencode/peakguard-opencode.js` reexporta ambos componentes.
- **Identidad Compuesta e Instancia (T18.1.2, T18.2.3):** Cada instancia genera `CLIENT_INSTANCE_ID = opencode_inst_<pid>_<random>`. Las sesiones se indexan internamente como `client_type:client_instance_id:session_id`. Se exige `client_instance_id` obligatorio en acuses Stop ACK.
- **Validación Estricta de Turnos (T18.2.2):** Si se recibe una orden Stop tardía cuyo `turn_id` no coincide con el turno actualmente activo (`turnID !== currentSess.turnID`), la orden es descartada sin abortar el nuevo turno y se envía Stop ACK con error `turn_mismatch`. Si la identidad es inverificable o inactiva, se delega al fallback del gateway.
- **Proveedor Desconocido Neutral (T18.2.1):** No se asigna `deepseek_direct` por defecto. Si el modelo o proveedor no especifica DeepSeek, se reporta como `unknown`.
- **Recuperación de Órdenes Stop (T18.1.3, T18.2.4):** Al conectarse o reconectarse tras un corte, consulta `GET /api/v1/adapters/commands/pending?client_instance_id=...` para ejecutar cualquier orden de detención emitida durante la desconexión.
- **Reconexión y Aislamiento (T18.1.6):** Backoff exponencial persistente entre intentos fallidos; se reinicia solo tras 5 segundos de conexión continua estable. Guardia booleana contra reconexiones simultáneas. Si la sesión no pertenece a la instancia, la orden es ignorada sin interrumpir procesos ajenos.
- **Latidos Periódicos (T18.1.4):** Durante turnos activos (`message.created`), emite latidos cada 10s para mantener viva la sesión ante turnos de razonamiento extensos (>30 min), y detiene el temporizador en `message.completed` o `session.idle` notificando `is_active: false`.
- **Control nativo verificado:** `client.session.abort()` cancela exclusivamente el turno activo, preservando historial y pestañas.
- **Enrutamiento por Gateway:** Paquete `@ai-sdk/openai-compatible` apuntando a `http://127.0.0.1:8765/v1`.
- **Validación:** Probado con `adapters/tests/adapter-mock-runner.test.mjs`, `adapters/tests/t18_1_repro_adapter_tests.mjs` y `adapters/tests/t18_2_repro_adapter_tests.mjs` bajo runtime Node.js.

### 3.2. Pi (`0.87.1`)
- **Detección y seguimiento:** Extensión `adapters/pi/peakguard-pi.ts` registra sesiones activas en `turn_start` con `client_instance_id` estable y actualiza estado con `is_active: false` en `agent_settled`. No asigna `deepseek_direct` por defecto si el modelo es desconocido (T18.2.1).
- **Validación Estricta de Turnos (T18.2.2):** Valida que `event.turn_id === activeTurnId`. Si difiere, descarta la orden tardía, envía acuse con error `turn_mismatch` y evita cancelar el nuevo turno. Si no hay turno activo, delega en el gateway.
- **Reconexión Estable y Backoff (T18.2.5):** No reinicia `reconnectAttempts` inmediatamente al recibir HTTP 200 en SSE; utiliza `stableTimer` de 5 segundos de conexión continua ininterrumpida antes de resetear a 0. Incluye bandera `isConnecting` y destrucción limpia de sockets activos para evitar bucles concurrentes.
- **Autenticación (T18, T18.1.5):** Lee credencial de `~/.peakguard/control.token` o `PEAKGUARD_CONTROL_TOKEN` y transmite únicamente en cabecera HTTP Bearer. Falla cerrado si el archivo de token está vacío o corrupto.
- **Indicador UI Verificado (T18.1.1):** Solo muestra `⛔ PEAK (Protegido)` si `status.coverage === "PROTECTED"`. Si la cobertura es `PARTIAL`, `UNVERIFIED` o `UNCOVERED`, muestra `⛔ PEAK (<COBERTURA>)` reflejando de forma honesta y conservadora la protección real.
- **Control nativo y Aislamiento (T18.1.2, T18.1.3):** Invocación de `activeCtx.abort()` o `pi.abort()`. El turno se cancela sin cerrar la consola interactiva ni ejecutar `shutdown()`. Se valida estrictamente `event.client_instance_id` y `event.session_id === activeSessionId`. Incluye recuperación de órdenes pendientes en `/api/v1/adapters/commands/pending`.
- **Latidos Periódicos (T18.1.4):** Intervalo de 10s mientras el turno permanezca activo, deteniéndose en `agent_settled`.
- **Enrutamiento por Gateway:** `~/.pi/agent/models.json` con `"baseUrl": "http://127.0.0.1:8765/v1"`.

### 3.3. Hermes (`0.21.4`)
- **Detección de modelo/proveedor:** `config.yaml` define `model.provider` y `model.default`.
- **Control nativo:** El comando `/stop` cancela la ejecución, pero puede abortar cadenas de subagentes o procesos asociados en background. Se mantiene como **cancelación de stream por gateway + alerta** como mecanismo seguro primario hasta aislar el comando de parada granular.
- **Enrutamiento por Gateway:** Inmediato; `config.yaml` incluye `model.base_url: http://127.0.0.1:8765/v1`.

---

## 4. Clasificación Dinámica de Cobertura (T18, T18.1 y T18.2)

- **PROTEGIDO (`PROTECTED`)**: Tráfico activamente observado pasando a través del Gateway loopback (`127.0.0.1:8765`) registrado por el `TrafficTracker` interno para DeepSeek (`direct_traffic_observed == true`). **Registrar una sesión activa de DeepSeek sin tráfico intermediado por el gateway NO otorga cobertura PROTECTED (T18.2.1); se reporta como `PARTIAL`**.
- **PARCIAL (`PARTIAL`)**: Proveedor habilitado y ruta configurada en el archivo TOML o sesión activa registrada en espera de observar tráfico real intermediado por el gateway.
- **NO VERIFICADO (`UNVERIFIED`)**: Proveedores o configuraciones donde el cliente gestiona la inferencia internamente sin enrutar al Gateway local (ej. OpenCode Go por defecto sin custom provider configurado).
- **NO CUBIERTO (`UNCOVERED`)**: Procesos o proveedores deshabilitados explícitamente en la configuración (`enabled = false`).
- **DESCONECTADO (`DISCONNECTED`)**: Demonio PeakGuard no responde o no está en ejecución. Nunca se reporta PROTECTED en este estado.
