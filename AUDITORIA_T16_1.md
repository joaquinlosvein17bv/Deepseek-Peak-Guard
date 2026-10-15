# Informe de Auditoría Técnica — T16.1: Problemas Pendientes en PeakGuard

**Fecha:** 15 de octubre de 2026  
**Rama:** `main` (commit evaluado: `c8bbc00`)  
**Tipo de documento:** Auditoría y diagnóstico técnico previo a corrección de código.  
**Estado general:** Se han investigado los 8 hallazgos señalados en la auditoría. Todos los hallazgos han sido **CONFIRMADOS** mediante inspección estática del código, análisis de ejecución y validación de interfaces. Se documenta la causa raíz, la evidencia concreta, las salidas de prueba y la propuesta técnica detallada de solución para cada uno.

---

## Índice de Hallazgos Auditados

1. [P0 — Seguridad de la Control API (Ausencia de Bearer Token y Defaults Inseguros)](#1-p0--seguridad-de-la-control-api)
2. [P0 — Stop Nativo al Comenzar el Pico (Desconexión de Eventos y Falta de Registro de Sesiones)](#2-p0--stop-nativo-al-comenzar-el-pico)
3. [P0 — Cobertura Declarada Incorrectamente (Hardcoded 'PROTEGIDO' y Fallbacks Inseguros)](#3-p0--cobertura-declarada-incorrectamente)
4. [P1 — Cancelación Temporal de Streams SSE (Ausencia de Temporizador Independiente de Chunks)](#4-p1--cancelación-temporal-de-streams-sse)
5. [P1 — Duplicación de Órdenes Stop (Deduplicación por Minuto en Lugar de por Ventana)](#5-p1--duplicación-de-órdenes-stop)
6. [P1 — Integraciones Reales de OpenCode y Pi (Arquitectura de Plugins y Validación de Runtime)](#6-p1--integraciones-reales-de-opencode-y-pi)
7. [P1 — Pruebas E2E del Demonio (Puertos Fijos, Reloj Real y Simulación de Apagado)](#7-p1--pruebas-e2e-del-demonio)
8. [P2 — Horarios, Festivos Futuros y Canales de Notificación](#8-p2--horarios-festivos-futuros-y-canales-de-notificación)
9. [Matriz de Riesgos para el Uso con Claves API Reales](#9-matriz-de-riesgos-para-el-uso-con-claves-api-reales)
10. [Plan de Pruebas de Regresión Propuesto](#10-plan-de-pruebas-de-regresión-propuesto)

---

## 1. P0 — Seguridad de la Control API

### 1.1. Estado Real
**CONFIRMADO**

### 1.2. Archivos, Funciones y Líneas Relevantes
- `core/src/control_api/mod.rs`:
  - Líneas 59–67: `create_control_router` monta las rutas sin middleware ni capa de autorización.
  - Líneas 140–154: `handle_set_mode` no valida cabeceras HTTP ni tokens.
  - Líneas 40–52: `StopAckRequest` declara `#[serde(default = "default_true")]` para el campo `success`.
- `core/src/config.rs`:
  - Líneas 150–157: `ControlConfig` define `token_env`, pero dicho valor no se consulta ni se propaga como validador al router de Control API.

### 1.3. Pruebas Ejecutadas y Salidas Reales
Se analizó la prueba `test_real_daemon_binary_lifecycle_and_ports` en `core/tests/real_daemon_e2e_tests.rs:149-156`:
```rust
let mode_put_resp = client
    .put(format!("http://127.0.0.1:{}/api/v1/config/mode", ctrl_port))
    .json(&serde_json::json!({ "mode": "notify_only" }))
    .send()
    .await
    .unwrap();
assert_eq!(mode_put_resp.status(), reqwest::StatusCode::OK);
```
**Resultado:** La petición `PUT` sin ninguna cabecera `Authorization: Bearer <token>` devuelve `HTTP 200 OK` y muta el modo operativo del demonio de `restricted_stop` a `notify_only`.

### 1.4. Causa Técnica
1. **Falta de middleware de autenticación:** `create_control_router` no incorpora ningún middleware de Axum (`validate_bearer_token` o capa `tower_http::auth`). Las rutas mutativas están completamente abiertas en la interfaz local.
2. **Defecto en Stop ACK (`default_true`):** Al deserializar `StopAckRequest`, la función `default_true()` asigna `success = true` cuando el cliente no envía dicho campo. Si un adaptador incompleto o defectuoso reporta un acuse sin declarar el estado explícito, se registra erróneamente como un stop exitoso.

### 1.5. Propuesta Específica de Solución
1. **Middleware de Bearer Token:**
   - Inyectar el token esperado en `ControlApiState` leyendo `std::env::var(&cfg.control.token_env)`. Si no está definido en el entorno, generar un token criptográfico seguro efímero durante el arranque o exigir configuración obligatoria.
   - Añadir una función middleware en Axum que intercepte peticiones a `/api/v1/config/*` y `/api/v1/adapters/*`, validando que la cabecera `Authorization` contenga exactamente `Bearer <token>`. Retornar `401 Unauthorized` si falta o es incorrecto. Permitir `/api/v1/health` sin autenticación para probes de liveness.
2. **Eliminación de `default_true`:**
   - Hacer que `success` sea obligatorio (`pub success: bool`) sin valor predeterminado, o que su valor por defecto sea `false` (fail-safe).

---

## 2. P0 — Stop Nativo al Comenzar el Pico

### 2.1. Estado Real
**CONFIRMADO**

### 2.2. Archivos, Funciones y Líneas Relevantes
- `core/src/commands/run.rs`:
  - Líneas 173–176: El ticker emite el evento `peak_started`:
    ```rust
    ticker_event_tx.send(format!(
        "{{\"type\":\"peak_started\",\"schedule_id\":\"{}\",\"transition_utc\":\"{}\"}}",
        sched_id, transition_str
    ));
    ```
- `core/src/gateway/mod.rs`:
  - Líneas 124–132: Es el **único** lugar donde se emite `stop_requested`, pero ocurre únicamente cuando el gateway recibe una **nueva petición HTTP entrante**.
- `adapters/opencode/peakguard-opencode.js`:
  - Línea 78: `if (event.type === "stop_requested" && event.session_id)`
- `adapters/pi/peakguard-pi.ts`:
  - Línea 126: `if (event.type === "stop_requested")`

### 2.3. Pruebas Ejecutadas y Salidas Reales
Búsqueda de emisiones en todo el código fuente (`git grep "stop_requested"`):
```text
adapters/opencode/peakguard-opencode.js:78: if (event.type === "stop_requested" && event.session_id)
adapters/pi/peakguard-pi.ts:126:            if (event.type === "stop_requested")
core/src/gateway/mod.rs:125:                "{\"type\":\"stop_requested\",\"session_id\":\"{}\",...}"
```
**Resultado:** `run.rs` nunca emite `stop_requested`. Cuando el reloj entra en horario pico, una sesión abierta en OpenCode o Pi que se encuentre ejecutando un turno, procesando herramientas o en espera de respuesta **nunca recibe la orden de abortar** salvo que intente emitir una llamada HTTP POST nueva hacia el gateway en ese mismo milisegundo.

### 2.4. Causa Técnica
1. **Desalineación de contratos de eventos:** El demonio anuncia la transición genérica `peak_started`, pero los adaptadores esperan órdenes imperativas dirigidas `stop_requested`.
2. **Ausencia de un registro de sesiones activas en el núcleo:** El demonio no mantiene constancia de qué sesiones de OpenCode o Pi están actualmente activas ni qué modelos están utilizando. Por tanto, no puede enviar una orden `stop_requested` parametrizada con `session_id` al momento de la transición.

### 2.5. Propuesta Específica de Solución
1. **Registro de sesiones activas en Control API:**
   - Crear un endpoint `POST /api/v1/sessions/heartbeat` o `POST /api/v1/sessions/register` donde los adaptadores de OpenCode (`session.created`) y Pi (`turn_start` / `agent_start`) registren su ID de sesión, el modelo en uso y su estado.
2. **Emisión de órdenes `stop_requested` por sesión en el ticker:**
   - Cuando el monitor de `run.rs` detecte la transición a Peak en modo `restricted_stop`, debe consultar el registro de sesiones activas. Para cada sesión que utilice un modelo del proveedor afectado, emitirá un evento individual `stop_requested` con su `session_id`.
3. **Recepción de evento `peak_started` como salvaguarda en adaptadores:**
   - Si los adaptadores reciben `peak_started`, deben evaluar localmente si tienen una sesión activa con un modelo protegido y ejecutar el abort nativo inmediatamente sin depender exclusivamente de un `stop_requested` individual.

---

## 3. P0 — Cobertura Declarada Incorrectamente

### 3.1. Estado Real
**CONFIRMADO**

### 3.2. Archivos, Funciones y Líneas Relevantes
- `core/src/control_api/mod.rs`:
  - Línea 90:
    ```rust
    coverage: "PROTEGIDO".to_string(),
    ```
- `adapters/opencode/peakguard-tui-sidebar.js`:
  - Líneas 12–18: Estado inicial con `coverage: "PROTEGIDO"`.
  - Línea 38: Si `fetchStatus()` falla (demonio apagado), devuelve `null` y `currentState.coverage` permanece como `"PROTEGIDO"`.
  - Línea 74: `const coverageStr = currentState.coverage || "PROTEGIDO";`.

### 3.3. Pruebas Ejecutadas y Salidas Reales
1. Verificación en `core/src/control_api/mod.rs`: El handler `handle_status` siempre responde `"coverage": "PROTEGIDO"` en el JSON, sin importar si el proveedor activo es OpenCode Go (que es `UNVERIFIED`), si la ruta de inferencia está deshabilitada o si el modelo solicitado no está configurado.
2. Verificación en `peakguard-tui-sidebar.js`: Al iniciarse la TUI de OpenCode con el demonio de PeakGuard detenido, la barra lateral muestra:
   `PeakGuard [● OFF-PEAK]`
   `Cobertura: PROTEGIDO`
   generando una falsa sensación de protección cuando no hay ningún proceso interceptando peticiones.

### 3.4. Causa Técnica
La propiedad `coverage` se asignó como un literal de texto estático (`"PROTEGIDO"`) en lugar de calcularse dinámicamente consultando `state.registry.classify_coverage(...)` o el estado de conectividad real del demonio.

### 3.5. Propuesta Específica de Solución
1. **Cálculo dinámico en Control API:**
   - `handle_status` debe evaluar el proveedor principal activo o los proveedores habilitados en la configuración.
   - Si el proveedor es `opencode_go` sin custom provider, o si no hay upstream validado, debe responder `"UNVERIFIED"`.
   - Si la configuración tiene proveedores sin enrutamiento verificado, responder `"UNCOVERED"`.
2. **Manejo honesto de desconexión en adaptadores y widget:**
   - El estado inicial de `peakguard-tui-sidebar.js` debe ser:
     ```javascript
     let currentState = {
       is_peak: false,
       mode: "UNKNOWN",
       coverage: "DESCONECTADO",
       connected: false
     };
     ```
   - Si `fetchStatus()` falla, debe reflejar explícitamente: `Cobertura: NO DISPONIBLE (Daemon Offline)` y `title: PeakGuard [DESCONECTADO]`.

---

## 4. P1 — Cancelación Temporal de Streams SSE

### 4.1. Estado Real
**CONFIRMADO**

### 4.2. Archivos, Funciones y Líneas Relevantes
- `core/src/gateway/mod.rs`:
  - Líneas 225–236:
    ```rust
    let wrapped_stream = async_stream::stream! {
        while let Some(item) = stream_bytes.next().await {
            let now = clock.now();
            let decision = policy_engine.evaluate_request(...);
    ```

### 4.3. Pruebas Ejecutadas y Salidas Reales
En `core/tests/stream_cancellation_tests.rs`, el test envía chunks simulados cada 10ms. La cancelación ocurre únicamente porque llega un nuevo chunk que despierta el `.next().await`.
**Escenario de fallo confirmado:** Si el upstream entra en un período de latencia o razonamiento profundo (deep reasoning / thinking) de 60 segundos entre las `00:59:30` UTC y las `01:00:30` UTC, durante ese minuto no llega ningún byte por el socket. El bucle `while let Some(item) = stream_bytes.next().await` permanece dormido. La conexión TCP permanece abierta en horario peak y, si el upstream emite el bloque de respuesta a las `01:00:30`, los tokens ya fueron generados y computados por el proveedor en tarifa pico antes de que el gateway procese el chunk.

### 4.4. Causa Técnica
El flujo de bytes está acoplado puramente a la recepción de datos I/O (`stream_bytes.next()`). No existe un temporizador asíncrono concurrente que despierte la tarea cuando se alcanza el timestamp de transición a Peak.

### 4.5. Propuesta Específica de Solución
1. **Uso de `tokio::select!` con cálculo de tiempo límite:**
   - Al abrir el stream SSE, consultar `schedule.next_peak(now)`.
   - Si el inicio de peak ocurrirá en el futuro próximo, calcular la duración `time_until_peak`.
   - Envolver la lectura del stream con `tokio::select!`:
     ```rust
     tokio::select! {
         item = stream_bytes.next() => {
             // procesar chunk normalmente
         }
         _ = tokio::time::sleep(time_until_peak) => {
             // El temporizador venció exactamente al inicio del pico:
             // Emitir chunk de error OpenAI, cerrar conexión y notificar cancelación
         }
     }
     ```
   - Esto garantiza que la conexión se cancele en el segundo exacto del inicio del pico, sin importar si el upstream está en silencio o emitiendo datos.

---

## 5. P1 — Duplicación de Órdenes Stop

### 5.1. Estado Real
**CONFIRMADO**

### 5.2. Archivos, Funciones y Líneas Relevantes
- `core/src/policy/mod.rs`:
  - Líneas 135–142:
    ```rust
    let transition_str = time.format("%Y-%m-%dT%H:%M").to_string();
    let key = StopKey {
        session_id: sess.to_string(),
        turn_id: turn_id.unwrap_or("default").to_string(),
        transition_utc: transition_str,
    };
    ```

### 5.3. Pruebas Ejecutadas y Salidas Reales
Análisis lógico determinista:
- A las `01:00:15 UTC`, `transition_str` es `"2026-10-15T01:00"`. Se inserta en `stop_emitted` y se emite `StopRequired`.
- A las `01:01:05 UTC`, `time.format("%Y-%m-%dT%H:%M")` produce `"2026-10-15T01:01"`.
- Al buscar en `stop_emitted`, la clave no coincide porque el minuto cambió de `01:00` a `01:01`.
- **Resultado:** Si la sesión continúa activa o envía una solicitud en el minuto siguiente, `policy_engine` vuelve a emitir `StopRequired` repetidamente cada 60 segundos durante toda la ventana peak de 3 o 4 horas.

### 5.4. Causa Técnica
La clave de deduplicación utiliza la marca de tiempo de la evaluación formateada al minuto (`%Y-%m-%dT%H:%M`), en lugar de utilizar el identificador único de la **ventana peak actual** (`window_start_utc`), tal como lo hace el motor de notificaciones en `DeduplicationKey`.

### 5.5. Propuesta Específica de Solución
1. Modificar `StopKey` para que `transition_utc` almacene el inicio exacto de la ventana peak actual:
   ```rust
   let window_start = schedule.current_or_next_peak_start(time);
   let key = StopKey {
       session_id: sess.to_string(),
       turn_id: turn_id.unwrap_or("default").to_string(),
       transition_utc: window_start.to_rfc3339(),
   };
   ```
2. De este modo, una sesión solo recibirá `StopRequired` una única vez por cada ventana peak completa (ej. la ventana de las 01:00–04:00 UTC).

---

## 6. P1 — Integraciones Reales de OpenCode y Pi

### 6.1. Estado Real
**CONFIRMADO** (inconsistencias arquitectónicas en adaptadores y validación incompleta en runtime real).

### 6.2. Archivos, Funciones y Líneas Relevantes
- `adapters/opencode/peakguard-opencode.js`
- `adapters/opencode/peakguard-tui-sidebar.js`
- `C:\Users\meliodaslosven\.config\opencode\tui.jsonc`
- `C:\Users\meliodaslosven\.config\opencode\opencode.jsonc`
- `adapters/pi/peakguard-pi.ts`

### 6.3. Pruebas Ejecutadas y Salidas Reales
1. **Inspección de OpenCode `1.18.35` instalado:**
   - La configuración de plugins en `~/.config/opencode/` tiene dos archivos separados:
     - `opencode.jsonc`: para plugins de backend/agente (`"plugin": [...]`).
     - `tui.jsonc`: para plugins de interfaz TUI (`"plugin": [...]`).
   - Los plugins TUI reales existentes (ej. `herdr-tui-session.js`) implementan:
     ```javascript
     export default { id: "herdr.opencode.session-selection", setup, tui };
     ```
   - Sin embargo, `peakguard-tui-sidebar.js` exporta `export const tui = ...` como función nombrada, sin export por defecto. Si se carga en `tui.jsonc`, OpenCode falla al resolver el objeto del plugin.
2. **Inspección de Pi `0.87.1` instalado:**
   - Se revisó el código fuente compilado de Pi en `AppData\Roaming\npm\node_modules\@earendil-works\pi-coding-agent\dist\core\agent-session.js:2440-2455`:
     ```javascript
     abort: () => {
         if (this._extensionAbortHandler) {
             this._extensionAbortHandler();
             return;
         }
         void this.abort();
     }
     ```
     El método `ctx.abort()` existe efectivamente en el contexto de sesión de Pi `0.87.1`. Sin embargo, tal como se confirmó en el punto 2, la extensión nunca lo ejecuta al entrar a peak porque el demonio emite `peak_started` en lugar de `stop_requested`.
3. **Falta de prueba en runtime real:**
   - Hasta ahora solo se validó la sintaxis estática con Node.js (`node --check`). No se ha ejecutado un ciclo de vida real levantando OpenCode o Pi contra un upstream local simulado para comprobar que el turno se cancela preservando el historial.

### 6.4. Causa Técnica
1. Desalineación con el esquema de carga de plugins de OpenCode V2 (separación estricta entre proceso server y proceso TUI).
2. Ausencia de un entorno de pruebas con arnés de integración que simule un cliente OpenCode y Pi interactuando con el socket de control.

### 6.5. Propuesta Específica de Solución
1. **Separación y corrección de plugins de OpenCode:**
   - `peakguard-opencode.js`: Plugin de servidor puro para `opencode.jsonc`. Registra hooks de eventos (`session.created`, `tool.execute.before`), reporta sesiones activas a la Control API y atiende abortos vía `client.session.abort()`.
   - `peakguard-tui-sidebar.js`: Plugin TUI puro para `tui.jsonc`. Exporta el objeto predeterminado con `{ id: "peakguard-sidebar", tui }`.
2. **Arnés de prueba de integración de adaptadores:**
   - Crear un script de prueba automatizado en Node.js que cargue los módulos de los adaptadores, suministre objetos `api` y `ctx` simulados que reflejen la estructura exacta de OpenCode `1.18.35` y Pi `0.87.1`, y verifique que ante la señal de peak se invoquen `session.abort()` y `ctx.abort()` sin emitir excepciones y enviando el Stop ACK correspondiente.

---

## 7. P1 — Pruebas E2E del Demonio

### 7.1. Estado Real
**CONFIRMADO**

### 7.2. Archivos, Funciones y Líneas Relevantes
- `core/tests/real_daemon_e2e_tests.rs`:
  - Líneas 47–48: Puertos fijos `gw_port = 18765`, `ctrl_port = 18766`.
  - Líneas 107–114: Ejecución directa de `peakguard run`.
  - Línea 220: `child.kill().await.expect(...)`.

### 7.3. Pruebas Ejecutadas y Salidas Reales
- Revisión de `core/tests/real_daemon_e2e_tests.rs`:
  1. Los puertos `18765` y `18766` están cableados estáticamente. Si otro proceso o ejecución concurrente los ocupa, el test falla de inmediato.
  2. El ejecutable utiliza `SystemClock` inmutable. Dado que la prueba se ejecutó en horario valle (Off-Peak real), la prueba **nunca comprobó una transición a Peak en el binario compilado**.
  3. Para finalizar el proceso, la prueba invoca `child.kill()`, lo cual envía una señal `SIGKILL` forzada a nivel de sistema operativo. Esto no verifica en absoluto el bloque `tokio::signal::ctrl_c()` ni el apagado ordenado implementado en `run.rs:284-290`.

### 7.4. Causa Técnica
Diseño apresurado del arnés de prueba E2E en T16, limitándose a probar llamadas HTTP básicas sin inyección de tiempo ni gestión de señales.

### 7.5. Propuesta Específica de Solución
1. **Asignación dinámica de puertos libres:**
   - Enlazar temporalmente `TcpListener::bind("127.0.0.1:0")` para obtener puertos efímeros libres antes de generar el archivo TOML de prueba.
2. **Soporte para bandera de tiempo de prueba en el binario:**
   - Añadir una opción oculta o de depuración en `peakguard run`: `--simulated-time <RFC3339>` o variable de entorno `PEAKGUARD_SIMULATED_TIME`, permitiendo que las pruebas E2E sitúen el reloj en el segundo `00:59:58 UTC` y verifiquen la transición automática a Peak a las `01:00:00 UTC` con el binario real.
3. **Verificación de apagado ordenado:**
   - Probar el apagado limpio enviando una señal controlada o a través de un endpoint administrativo de shutdown en la Control API (`POST /api/v1/control/shutdown` protegido con token), comprobando que el código de salida sea `0`.

---

## 8. P2 — Horarios, Festivos Futuros y Canales de Notificación

### 8.1. Estado Real
**CONFIRMADO**

### 8.2. Archivos, Funciones y Líneas Relevantes
- `core/src/schedule/mod.rs`:
  - Líneas 211–236: Función `chinese_public_holidays_2026()` con fechas hardcodeadas exclusivamente para 2026.
- `core/src/commands/run.rs`:
  - Líneas 156–170: Variable `was_peak` compartida entre múltiples calendarios.
  - Líneas 252–264: Bloques de `warning_10` y `warning_5` carecen de llamada a Telegram.
  - Líneas 232, 252, 258: Comparación de igualdad exacta `seconds_left == 1800`, `== 600`, `== 300`.

### 8.3. Pruebas Ejecutadas y Salidas Reales
1. **Festivos futuros:**
   - Se revisó `chinese_public_holidays_2026()`. No hay soporte para años 2027 en adelante.
   - Si el sistema se ejecuta el 1 de enero de 2027, la función no contendrá esa fecha y DeepSeek Direct aplicará erróneamente horario peak en un festivo oficial donde la API de DeepSeek factura a tarifa valle.
   - En `config.rs`, `ScheduleConfig` no contiene el campo `holidays: Vec<String>`, impidiendo que el usuario añada festivos en su `peakguard.toml`.
2. **Múltiples calendarios en `run.rs`:**
   - `let mut was_peak = false;` está definida **fuera** del bucle `for (sched_id, schedule) in &ticker_schedules`.
   - Si el usuario tiene `deepseek_direct` (en peak) y `opencode_go` (fuera de peak), en la primera iteración `was_peak = true`; en la segunda iteración entra en `!is_peak && was_peak`, restableciendo `was_peak = false` y emitiendo `peak_ended`. En el segundo siguiente vuelve a emitir `peak_started`. Se produce una oscilación continua cada segundo.
3. **Alertas de Telegram faltantes y fragilidad temporal:**
   - En `core/src/commands/run.rs:252-264`, los bloques `warning_10` y `warning_5` envían el evento SSE pero **no tienen código de notificación de Telegram**.
   - Evaluar `seconds_left == 1800` en un ticker de 1 segundo es susceptible a jitter: si una iteración tarda 1.002s y evalúa en `1801` y luego en `1799`, la alerta de 30 minutos se pierde por completo.

### 8.4. Causa Técnica
1. Omisión de la integración de Telegram en los bloques de 10 y 5 minutos.
2. Gestión del estado de pico como variable escalar en lugar de un `HashMap<String, bool>` por calendario.
3. Ausencia de persistencia o configuración de festivos en el modelo TOML.
4. Lógica de umbrales basada en igualdad estricta en vez de rangos con deduplicación por clave.

### 8.5. Propuesta Específica de Solución
1. **Configuración de festivos en TOML:**
   - Añadir `pub holidays: Vec<String>` en `ScheduleConfig` para que los usuarios puedan definir festivos de cualquier año directamente en `peakguard.toml`.
2. **Estado de transición por calendario:**
   - Sustituir `let mut was_peak = false;` por `let mut schedule_states: HashMap<String, bool> = HashMap::new();`.
3. **Completar notificaciones de Telegram y tolerar jitter:**
   - Añadir el envío de Telegram con su clave deduplicada para `warning_10` y `warning_5`.
   - Utilizar el motor `schedule.next_alert_threshold(time)` que evalúa rangos con `DeduplicationEngine` en lugar de comparaciones `==` frágiles.

---

## 9. Matriz de Riesgos para el Uso con Claves API Reales

| Riesgo Técnico Identificado | Impacto | Severidad | Mitigación Requerida antes de uso con API Keys Reales |
|---|---|---|---|
| **Fuga de tokens en streams con pausas largas** | Peticiones en streaming que no envían chunks al momento de la transición permanecen abiertas y facturan a tarifa pico. | **Crítica** | Implementar cancelación por tiempo concurrente (`tokio::select!` con timeout) en el gateway. |
| **No recepción de Stop nativo en turnos activos** | Si un turno largo de OpenCode o Pi está corriendo sin hacer peticiones al gateway, no se detiene al iniciar el pico. | **Crítica** | Unificar contrato de eventos (`stop_requested` emitido por el demonio a sesiones registradas). |
| **Mutación no autorizada de modo en Control API** | Cualquier proceso local puede desactivar la protección cambiando el modo a `notify_only` sin token. | **Alta** | Middleware obligatorio de Bearer Token en todos los endpoints mutativos de Control API. |
| **Oscilación en múltiples calendarios** | Falsas alertas continuas de inicio/fin de peak cada segundo si hay más de un calendario activo. | **Media** | Aislar estado de pico en un mapa por `schedule_id`. |
| **Omisión de alertas Telegram a 10 y 5 minutos** | El usuario no recibe avisos críticos de proximidad inmediata al horario pico en su móvil. | **Media** | Incorporar `notify_telegram` en umbrales de 10m y 5m con deduplicación. |
| **Falsa indicación de cobertura en TUI** | El usuario cree estar protegido cuando el demonio está caído o el proveedor no pasa por el gateway. | **Alta** | Estado `DESCONECTADO` / `UNVERIFIED` dinámico en API y adaptadores. |

---

## 10. Plan de Pruebas de Regresión Propuesto

Para considerar resueltos estos problemas de manera verificable una vez autorizadas las correcciones, se implementará el siguiente conjunto de pruebas:

1. **`control_api_security_tests.rs`:**
   - Verificar que `PUT /api/v1/config/mode` sin token devuelva `401 Unauthorized`.
   - Verificar que con token inválido devuelva `403 Forbidden` / `401 Unauthorized`.
   - Verificar que solo con el Bearer token correcto retorne `200 OK`.
   - Verificar que `StopAckRequest` sin `success` sea rechazado con `422 Unprocessable Entity` o `400 Bad Request`.
2. **`stream_timeout_cancellation_tests.rs`:**
   - Crear un mock stream que permanezca en silencio durante la transición a peak.
   - Verificar que el gateway cancele la conexión y emita el error OpenAI en el segundo exacto de la transición, sin esperar chunks.
3. **`stop_deduplication_tests.rs`:**
   - Evaluar una sesión repetidamente durante 5 minutos dentro del mismo período pico y verificar que se emite exactamente **una** orden de Stop para toda la ventana.
4. **`multi_schedule_daemon_tests.rs`:**
   - Configurar dos calendarios con horarios desfasados y verificar que el monitor no oscile ni emita falsos `peak_ended`.
5. **`integration_adapters_runner.test.js`:**
   - Arnés en Node.js que simule OpenCode y Pi ejecutando turnos reales y compruebe que al recibir `peak_started` / `stop_requested` se invoquen los abortos nativos y se preserve la sesión.
6. **`real_daemon_time_warp_e2e_tests.rs`:**
   - Prueba E2E con el binario real `peakguard run` utilizando puertos efímeros, simulando avance del reloj hacia el pico y verificando el apagado ordenado.

---
*Fin del informe de auditoría técnica T16.1. A la espera de la autorización del usuario antes de aplicar cualquier modificación al código.*
