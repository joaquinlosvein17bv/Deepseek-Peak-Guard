# 🛡️ PeakGuard (Deepseek-Peak-Guard)

> **Guardián local, ultraligero y determinista de horarios Peak / Off-Peak para modelos de IA (DeepSeek).**  
> Desarrollado en Rust nativo para Windows 10/11 x64, con soporte para OpenCode, Pi Coding Agent, Hermes Agent y servidor MCP opcional.

---

## 🎯 ¿Qué problema resuelve PeakGuard?

Los proveedores de modelos avanzados como DeepSeek implementan esquemas de tarifas dinámicas por horario en su API directa:
- **Horarios Peak (UTC):** `01:00 – 04:00` y `06:00 – 10:00` (días laborables de lunes a viernes).
- **Sobrecosto:** Durante estas franjas horarias las llamadas de inferencia tienen tarifas notablemente más elevadas que en horario valle (*off-peak*).
- **Riesgo en Agentes de Código:** Agentes autónomos como OpenCode, Pi o Hermes pueden ejecutar tareas complejas en segundo plano o cadenas de razonamiento prolongadas durante la noche o madrugada, incurriendo inadvertidamente en consumo excesivo durante horas pico.

**PeakGuard actúa como un escudo local inteligente:**
1. Conoce con precisión matemática y semántica de intervalos semicerrados `[start, end)` los horarios exactos en UTC.
2. Contempla el calendario oficial de festivos nacionales de China (en los cuales no aplica horario peak en la API directa).
3. Intercepta el tráfico saliente mediante un gateway local loopback (`127.0.0.1:8765`) y coordina paradas suaves (*graceful native abort*) con los editores sin jamás forzar el cierre violento del programa.

---

## ⚡ Características Principales

- **3 Modos de Operación Configurables:**
  - `strict_block`: Bloqueo inmediato en el gateway HTTP (retorna código `403 peakguard_policy_blocked`) y cancelación inmediata de streams SSE activos.
  - `restricted_stop`: Coordinación activa con los agentes. En OpenCode invoca `session.abort()` y en Pi `ctx.abort()`, preservando la sesión, el historial y las pestañas abiertas.
  - `notify_only`: Nunca bloquea ni interrumpe peticiones; únicamente emite advertencias y actualiza el contador.
- **Temporizador en Tiempo Real y Widget TUI:**
  - Widget para la barra lateral derecha de OpenCode (`adapters/opencode/peakguard-tui-sidebar.js`) con cuenta regresiva en vivo segundo a segundo.
  - Alertas preventivas configurables a **T−30 min**, **T−10 min** y **T−5 min**.
  - Cuenta regresiva crítica de **10 segundos** antes del inicio de la ventana peak.
- **Canal de Notificaciones Opcional (Telegram):**
  - Envío asíncrono no bloqueante de alertas directamente a tu bot/chat privado de Telegram.
  - Deduplicación estricta de notificaciones por clave `(schedule_id, window_start_utc, threshold, channel)`.
- **Servidor MCP Consultivo (`peakguard mcp`):**
  - Servidor JSON-RPC 2.0 sobre `stdio` para consultar estado, catálogo de modelos y cobertura desde cualquier entorno compatible con MCP.
  - **Solo lectura:** Protección contra auto-desbloqueo no autorizado por modelos LLM (§9.4).
- **Desacoplamiento Total:**
  - Si el servidor MCP o las alertas no están activos, el Gateway HTTP y el Control API mantienen el 100% de su capacidad protectora.

---

## 🏛️ Principios de Arquitectura y Seguridad

- **P1 — Nunca Matar Procesos:** Jamás se utiliza `taskkill`, `kill` o `process.exit()`. Las interrupciones se gestionan exclusivamente mediante las APIs nativas de aborto de turno del cliente.
- **P2 — Inmunidad de Terceros:** El tráfico dirigido a otros modelos (Anthropic Claude, OpenAI GPT-4o, Google Gemini, Ollama local) transita de forma transparente y sin interrupciones.
- **P3 — Perfiles Independientes:** DeepSeek Direct API y OpenCode Go tienen calendarios y reglas de festivos completamente aisladas.
- **P4 — Fallo Seguro (Fail-Closed):** Si se activa el modo estricto durante horario peak, no sale ninguna llamada desprotegida.
- **P5 — Cero Polling Inútil:** Sincronización basada en eventos Server-Sent Events (SSE) en el Control API (`127.0.0.1:8766`).
- **P6 — Privacidad Absoluta:** Jamás se registran ni almacenan claves de API, tokens de autorización, contenidos de prompts o respuestas en archivos de registro o salida de consola.

---

## 🚀 Instalación y Compilación Rápida

### Requisitos
- Windows 10/11 x64.
- Rust 1.85+ (compilado con toolchain MinGW GCC o MSVC).

### Compilación y Empaquetado Automático
Ejecuta el script de empaquetado incluido:
```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
```
El ejecutable optimizado se generará en:
```text
dist/
└── peakguard.exe   (6.5 MB, standalone, sin dependencias dinámicas externas)
```

---

## 💻 Uso de la Línea de Comandos (CLI)

```powershell
# Ver información de versión y arquitectura
peakguard version

# Ver estado actual del reloj UTC, modo y ventanas
peakguard status

# Ejecutar diagnóstico de puertos, configuración y clientes detectados
peakguard doctor

# Validar sintaxis y coherencia de un archivo TOML
peakguard config validate config\peakguard.example.toml

# Iniciar el demonio de PeakGuard (Gateway + Control API + Monitor)
peakguard run --config config\peakguard.example.toml

# Iniciar el servidor MCP interactivo sobre stdio
peakguard mcp --config config\peakguard.example.toml
```

---

## 🔌 Integración con Clientes

### 1. OpenCode
- **Plugin de Aborto Suave:** [`adapters/opencode/peakguard-opencode.js`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/opencode/peakguard-opencode.js)
  - Escucha el flujo SSE de `/api/v1/events` y ejecuta `client.session.abort({ path: { id } })` sin cerrar la aplicación.
- **Widget Lateral TUI:** [`adapters/opencode/peakguard-tui-sidebar.js`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/opencode/peakguard-tui-sidebar.js)
  - Muestra en tiempo real el modo, el estado y el temporizador en la barra lateral derecha.
- **Guía Completa:** [`adapters/opencode/README.md`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/opencode/README.md)

### 2. Pi Coding Agent
- **Extensión TypeScript:** [`adapters/pi/peakguard-pi.ts`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/pi/peakguard-pi.ts)
  - Se suscribe a transiciones peak y ejecuta `ctx.abort()` preservando la ventana y el contexto.
  - Actualiza el estado visual con `ctx.ui.setStatus()`.
- **Guía Completa:** [`adapters/pi/README.md`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/pi/README.md)

### 3. Hermes Agent
- **Integración y Monitor:** [`adapters/hermes/README.md`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/hermes/README.md) y [`adapters/hermes/peakguard-hermes-monitor.ps1`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/adapters/hermes/peakguard-hermes-monitor.ps1)
  - Protección perimetral a través de HTTP Gateway 403 para evitar que el comando global `/stop` interrumpa agentes delegados en segundo plano.

### 4. Servidor MCP
- **Documentación Completa:** [`docs/MCP.md`](file:///C:/Users/meliodaslosven/Desktop/Projects/deepseek/Deepseek-Peak-Guard/docs/MCP.md)
  - Configuración para Claude Desktop, OpenCode, Cursor y Windsurf.

---

## 🧪 Pruebas y Verificación

PeakGuard cuenta con una suite automatizada de **29 pruebas unitarias y de integración**, incluyendo:
- 100% de los casos límite de fixtures JSON (`test-fixtures/schedules/boundary_test_cases.json`).
- Detección de saltos bruscos en el reloj del sistema operativo (suspensión/reanudación de Windows).
- Pruebas E2E de gateway con servidor HTTP simulado y streaming concurrente (20 streams simultáneos).
- Validación de idempotencia y ciclos de deduplicación de alertas.
- Pruebas completas del servidor MCP y sus salvaguardas de seguridad.

Para ejecutar todas las pruebas:
```powershell
cd core
cargo test
```

Para ejecutar el smoke test de release:
```powershell
powershell -ExecutionPolicy Bypass -File scripts\smoke-test.ps1
```

---

## 📚 Documentación del Proyecto

- 📖 [Guía del Usuario y Manual de Despliegue](docs/USER_GUIDE.md)
- 🏗️ [Arquitectura del Sistema](docs/ARCHITECTURE.md)
- 🔒 [Modelo de Seguridad y Privacidad](docs/SECURITY.md)
- ⏱️ [Especificación de Horarios e Intervalos](docs/SCHEDULE_SPEC.md)
- 💻 [Matriz de Compatibilidad de Entorno](docs/COMPATIBILITY.md)
- 🧭 [Registro de Decisiones Arquitectónicas (ADRs)](docs/DECISIONS.md)
- 🛠️ [Guía de Compilación de Rust](docs/BUILD.md)
- 📡 [Informe de Viabilidad de OpenCode Go](docs/OPENCODE_GO.md)
- 🤖 [Manual del Servidor MCP](docs/MCP.md)
- 🗺️ [Backlog Futuro y Roadmap v1.1+](docs/FUTURE_BACKLOG.md)
- 📋 [Especificación y Plan Maestro Original](PeakGuard_Especificacion_Plan_Maestro.md)

---

## 📄 Licencia

Este proyecto está bajo la Licencia MIT. Consulta el archivo [LICENSE](LICENSE) para más detalles.
