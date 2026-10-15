# Matriz de Compatibilidad y Capacidades de Clientes y Proveedores

**Fecha de auditoría inicial:** 14 de octubre de 2026  
**Entorno de ejecución verificado:** Windows 11 Build 10.0.26200.0 (Win32NT) x64  
**Herramientas base:** Rust 1.99.0 (`cargo 1.99.0`), Node.js v24.13.1, Python 3.11.16

---

## 1. Inventario de Entorno y Clientes Instalados

| Aplicación / Herramienta | Versión Instalada | Método de Instalación / Ruta | Soporte Gateway HTTP | Soporte Abort / Stop Nativo | Estado de Verificación |
|---|---|---|---|---|---|
| **OpenCode** | `1.18.35` | `~/.gentle-ai/bin/opencode.ps1` (`npm/opencode`) | ✅ Sí (vía `@ai-sdk/openai-compatible` con `baseURL`) | ✅ Sí (`session.abort({ path })` / hooks de plugin) | `VERIFIED` |
| **Pi Coding Agent** | `0.87.1` | `@earendil-works/pi-coding-agent` (`npm/pi.ps1`) | ✅ Sí (`~/.pi/agent/models.json` con `baseUrl`) | ✅ Sí (`ctx.abort()`, no `ctx.shutdown()`) | `VERIFIED` |
| **Hermes Agent** | `0.21.4 (2026.9.21)` | Git (`AppData/Local/hermes/hermes-agent`) | ✅ Sí (`config.yaml` con `model.base_url`) | ⚠️ Condicionado (`/stop` detiene subprocesos) | `PARTIAL` |
| **Rust Toolchain** | `1.99.0 (b940084d7)` | `rustup` x86_64-pc-windows-msvc | N/A (Entorno de compilación) | N/A | `VERIFIED` |

---

## 2. Matriz de Proveedores y Modelos Facturables

| Proveedor | Endpoint Base Real | Modelos / Aliases Típicos | Horarios Peak (UTC) | Excepción Festivos Chinos | Soporte Routing Gateway |
|---|---|---|---|---|---|
| **DeepSeek Direct API** | `https://api.deepseek.com/v1` | `deepseek-chat`, `deepseek-reasoner` | Lun–Vie 01:00–04:00 y 06:00–10:00 UTC | ✅ Sí (oficial en pricing) | `VERIFIED` |
| **OpenCode Go** | `https://opencode.ai/zen/go/v1` | `opencode-go/deepseek-v4-flash`, `deepseek-v4.1-flash` | Lun–Vie 01:00–04:00 y 06:00–10:00 UTC | ❌ No especificada (tratar como NO) | `UNVERIFIED` (Inferencia nativa gestionada; requiere custom provider con baseURL local para ser `PROTECTED`) |
| **TokenHarbor / Resellers** | `https://tokenharbor.ai/v1` | `deepseek-v4.1-flash` | Dependiente del reseller (no inferir) | ❌ No especificada | `UNVERIFIED` (perfil propio requerido) |

---

## 3. Detalle de Compatibilidad por Cliente

### 3.1. OpenCode (`1.18.35`)
- **Detección de modelo/proveedor:** Mediante hooks de plugin en `~/.config/opencode/plugins/` (eventos `session.created`, `session.status`, `tool.execute.before`).
- **Control nativo:** La API del SDK expone `session.abort()`. El turno se cancela preservando la sesión y los mensajes previos en el historial de la TUI.
- **Enrutamiento por Gateway:**
  - El proveedor integrado `opencode/` almacena tokens en `~/.local/share/opencode/auth.json`.
  - Se puede definir un proveedor alternativo en `opencode.json` con paquete `@ai-sdk/openai-compatible` apuntando a `http://127.0.0.1:8765/v1`.
- **UI / Sidebar:** Soporta extensiones TUI vía `tui.jsonc` (slots de estado y statusline).

### 3.2. Pi (`0.87.1`)
- **Detección de modelo/proveedor:** Extensiones en `~/.pi/agent/extensions/`.
- **Control nativo:** Soporta `ctx.abort()`. La documentación advierte no usar `ctx.shutdown()`.
- **Enrutamiento por Gateway:** El archivo `~/.pi/agent/models.json` permite configurar cualquier proveedor con `"api": "openai-completions"` y `"baseUrl": "http://127.0.0.1:8765/v1"`.
- **UI:** Dispone de `ctx.ui.setStatus` y widgets compactos.

### 3.3. Hermes (`0.21.4`)
- **Detección de modelo/proveedor:** `config.yaml` define `model.provider` y `model.default`.
- **Control nativo:** El comando `/stop` cancela la ejecución, pero puede abortar cadenas de subagentes o procesos asociados en background. Se degrada a **cancelación de stream por gateway + alerta** como mecanismo seguro primario hasta aislar el comando de parada granular.
- **Enrutamiento por Gateway:** Inmediato; `config.yaml` incluye la clave `model.base_url: https://opencode.ai/zen/go/v1`, modificable a `http://127.0.0.1:8765/v1`.

---

## 4. Clasificación de Cobertura según la Especificación

- **PROTEGIDO (`PROTECTED` / `VERIFIED`)**: Tráfico configurado explícitamente para pasar por el Gateway `http://127.0.0.1:8765` y/o con adaptador nativo activo y verificado en tests.
- **PARCIAL (`PARTIAL`)**: Clientes donde el control nativo o el enrutamiento está sujeto a limitaciones (ej. Hermes con `/stop` amplio).
- **NO VERIFICADO (`UNVERIFIED`)**: Proveedores o configuraciones donde el cliente gestiona la inferencia internamente o no se ha verificado el enrutamiento a través del Gateway local (ej. OpenCode Go por defecto sin custom provider configurado).
- **NO CUBIERTO (`UNCOVERED`)**: Procesos o herramientas que apunten directamente a internet sin pasar por el loopback o sin plugin de control reportado.
