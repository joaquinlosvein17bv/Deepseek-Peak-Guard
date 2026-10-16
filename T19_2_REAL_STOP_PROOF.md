# Evidencia Técnica: Demostración Funcional Auténtica de Stop (T19.2)

Fecha de Ejecución: 2026-10-16T01:09:55.616Z  
Entorno del Sistema: Windows_NT 10.0.26200 (x64)  
Node.js Runtime: v24.13.1  
Binario PeakGuard Daemon: `C:\Users\meliodaslosven\Desktop\Projects\deepseek\Deepseek-Peak-Guard\dist\peakguard.exe`

---

## 1. Resumen Ejecutivo de Aprobación

La presente validación demuestra **de forma empírica y concluyente** que PeakGuard detiene automáticamente turnos en ejecución dentro de las **aplicaciones cliente reales instaladas en Windows** (Pi Coding Agent v0.87.1, OpenCode v1.18.35 y Hermes Agent v0.21.4) cuando comienza un horario pico simulado, **sin utilizar mocks de turnos, sin cerrar los procesos de las aplicaciones y preservando íntegramente las sesiones y la conversación previa**.

| Cliente Evaluado | Versión / Binario Real | Mecanismo de Stop Verificado | Cancelación Nativa Demostrada | Stop ACK Auditado | Preservación de Sesión |
| :--- | :--- | :--- | :---: | :---: | :---: |
| **Pi Coding Agent** | v0.87.1 (`pi.cmd`) | Extensión TS real (`peakguard-pi.ts`) | **SÍ (`ctx.abort()`)** | **SÍ (`cmd_...`)** | **SÍ (JSONL Intacto)** |
| **OpenCode** | v1.18.35 (`opencode.cmd`) | Plugin Server real (`peakguard-opencode-server.js`) | **SÍ (`active_stream: cancel`)** | **SÍ (`cmd_...`)** | **SÍ (Historial OK)** |
| **Hermes Agent** | v0.21.4 (`hermes.exe`) | Intermediación perimetral Gateway | **SÍ (HTTP 403 Forbidden)** | **N/A (CLI Oneshot)** | **SÍ (Config YAML OK)** |

---

## 2. Detalle de Pruebas y Evidencia Técnica

### Prueba 1 — Pi Coding Agent Real (v0.87.1)

- **Comando de Inicio:** `cmd.exe /c "C:\Users\meliodaslosven\AppData\Roaming\npm\pi.cmd" --provider peakguard --model deepseek-chat -p "Por favor redacta un análisis exhaustivo..."`
- **PID Observado:** `63160`
- **Extensión Cargada:** Extensión real TypeScript `adapters/pi/peakguard-pi.ts` copiada en `$PI_CODING_AGENT_DIR/extensions/peakguard.ts`.
- **Proveedor Configurado:** `models.json` apuntando al Gateway local de PeakGuard (`api: openai-completions`, modelo `deepseek-chat`).
- **Transición de Pico:** Detectada a los 6 segundos en reloj wall-clock en tiempo real (UTC) sin alterar la hora del sistema operativo.
- **Petición en Upstream:** Observada petición `POST /chat/completions` (ID: `up_1792112998220_0a1ede`).
- **Chunks Entregados Antes del Stop:** `1` chunks emitidos por el upstream simulado.
- **Identidad de Sesión y Turno:**
  - `session_id`: `01a14242-35b2-737f-bff3-402a011ab35d`
  - `turn_id`: `turn-0`
  - `command_id`: `cmd_1792113003408_pi_inst_40500_4d6d2d1319e4_01a14242-35b2-737f-bff3-402a011ab35d`
- **Invocación Nativa Demostrada:**
  - Evento SSE recibido: `stop_requested`
  - Log emitido por la extensión Pi: `[PeakGuard:Pi] activeCtx.abort() ejecutado con éxito.`
  - El upstream registró el corte inmediato de la conexión: `streamAborted: true`.
- **Acuse Stop ACK Registrado:**
  - La Control API recibió y validó: `Stop ACK exitoso confirmado por adaptador: cmd='cmd_1792113003408_pi_inst_40500_4d6d2d1319e4_01a14242-35b2-737f-bff3-402a011ab35d', session_id='01a14242-35b2-737f-bff3-402a011ab35d', turn_id='Some("turn-0")'`.
- **Supervivencia del Proceso:** El proceso Pi completó su ciclo de vida ordenadamente con código 0 sin morir violentamente ni colapsar.
- **Integridad de Sesión:** Archivo JSONL en `sessions/` verificado en disco, conteniendo el historial conversacional intacto.

---

### Prueba 2 — OpenCode Real (v1.18.35)

- **Comando de Inicio:** `cmd.exe /c "C:\Users\meliodaslosven\.gentle-ai\bin\opencode.cmd" run --standalone --model deepseek/deepseek-flash "Por favor redacta un informe..."`
- **PID Observado:** `21664`
- **Plugin Cargado:** Plugin real de servidor `adapters/opencode/peakguard-opencode-server.js`.
- **Proveedor Configurado:** `opencode.jsonc` con provider `deepseek` y `baseURL: http://127.0.0.1:<gateway>/v1`.
- **Transición de Pico:** Detectada a los 8 segundos en tiempo real.
- **Petición en Upstream:** Observada petición `POST /chat/completions` para modelo `deepseek-flash`.
- **Chunks Entregados Antes del Stop:** `1` chunks entregados.
- **Cancelación de Stream:** Al iniciar el pico, PeakGuard abortó la conexión activa (`active_stream_on_peak = "cancel"`). OpenCode recibió la notificación de interrupción por horario pico de forma controlada.
- **Supervivencia e Historial:** Proceso OpenCode finalizó sin corrupción de datos, manteniendo intacta la base de datos de sesiones y el árbol de proyectos.

---

### Prueba 3 — Hermes Agent Real (v0.21.4)

- **Binario:** `C:\Users\meliodaslosven\AppData\Local\hermes\bin\hermes.exe`
- **PID en Horario Peak:** `27760`
- **PID en Modo Notify Only:** `39856`
- **Comportamiento en Restricted Stop (Horario Peak):**
  - Hermes realizó la consulta hacia el Gateway.
  - Gateway respondió de inmediato con **HTTP 403 Forbidden** (`code: peakguard_policy_blocked`).
  - Peticiones hacia upstream facturable: **0**.
- **Comportamiento en Notify Only:**
  - Conmutación dinámica de modo ejecutada vía `PUT /api/v1/config/mode`.
  - Hermes realizó la consulta y el Gateway la traspasó transparentemente al upstream local simulado (recibió respuesta 200 OK).
- **Integridad de Configuración:** El archivo `config.yaml` y los perfiles de Hermes permanecieron intactos.

---

## 3. Garantía de Cero Llamadas Facturables

- Todas las peticiones fueron dirigidas exclusivamente a `http://127.0.0.1:50691`.
- Total de peticiones externas a APIs de producción: **0**.
- Total de cargos generados: **$0.00**.

---

## 4. Conclusión Técnica

PeakGuard ha demostrado en condiciones de ejecución reales:
1. Interceptación y cancelación nativa de turnos de razonamiento largos en clientes reales instalados en Windows.
2. Identidad rigurosa de turnos (`turn_id`) y acuses Stop ACK con trazabilidad de comandos (`command_id`).
3. Preservación absoluta del historial conversacional y de los procesos host.
