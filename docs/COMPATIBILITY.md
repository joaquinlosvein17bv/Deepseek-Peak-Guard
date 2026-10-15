# Matriz de Compatibilidad y Capacidades de Clientes y Proveedores

**Fecha de auditoría inicial:** 14 de octubre de 2026  
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
- **Control nativo verificado:** `client.session.abort()` cancela exclusivamente el turno activo, preservando historial y pestañas.
- **Aislamiento de Sesiones (T18):** Deduplicación de órdenes de stop y verificación de pertenencia en `activeSessions`.
- **Enrutamiento por Gateway:** Paquete `@ai-sdk/openai-compatible` apuntando a `http://127.0.0.1:8765/v1`.
- **Validación:** Probado con `adapters/tests/adapter-mock-runner.test.mjs` bajo runtime Node.js.

### 3.2. Pi (`0.87.1`)
- **Detección y seguimiento:** Extensión `adapters/pi/peakguard-pi.ts` registra sesiones activas en `turn_start` y actualiza estado con `is_active: false` en `agent_settled`.
- **Autenticación (T18):** Lee credencial de `~/.peakguard/control.token` o `PEAKGUARD_CONTROL_TOKEN` y transmite únicamente en cabecera HTTP Bearer (sin parámetros en URL). Reconexión con retroceso exponencial.
- **Control nativo y Aislamiento (T18):** Invocación de `activeCtx.abort()` o `pi.abort()`. El turno se cancela sin cerrar la consola interactiva ni ejecutar `shutdown()`. Se valida estrictamente `event.session_id === activeSessionId` para no interrumpir turnos ante órdenes dirigidas a otras sesiones.
- **Enrutamiento por Gateway:** `~/.pi/agent/models.json` con `"baseUrl": "http://127.0.0.1:8765/v1"`.

### 3.3. Hermes (`0.21.4`)
- **Detección de modelo/proveedor:** `config.yaml` define `model.provider` y `model.default`.
- **Control nativo:** El comando `/stop` cancela la ejecución, pero puede abortar cadenas de subagentes o procesos asociados en background. Se mantiene como **cancelación de stream por gateway + alerta** como mecanismo seguro primario hasta aislar el comando de parada granular.
- **Enrutamiento por Gateway:** Inmediato; `config.yaml` incluye `model.base_url: http://127.0.0.1:8765/v1`.

---

## 4. Clasificación Dinámica de Cobertura (T18)

- **PROTEGIDO (`PROTECTED`)**: Tráfico activamente observado pasando a través del Gateway loopback (`127.0.0.1:8765`) registrado por el `TrafficTracker` interno, o sesión activa conectada, autenticada y vinculada a dicho proveedor.
- **PARCIAL (`PARTIAL`)**: Proveedor habilitado y ruta configurada en el archivo TOML en espera de observar tráfico real en el gateway.
- **NO VERIFICADO (`UNVERIFIED`)**: Proveedores o configuraciones donde el cliente gestiona la inferencia internamente sin enrutar al Gateway local (ej. OpenCode Go por defecto sin custom provider configurado).
- **NO CUBIERTO (`UNCOVERED`)**: Procesos o proveedores deshabilitados explícitamente en la configuración (`enabled = false`).
- **DESCONECTADO (`DISCONNECTED`)**: Demonio PeakGuard no responde o no está en ejecución. Nunca se reporta PROTECTED en este estado.
