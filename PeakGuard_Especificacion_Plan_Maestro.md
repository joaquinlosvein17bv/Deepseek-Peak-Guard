# PEAKGUARD

## Especificación funcional, arquitectura técnica y plan maestro de implementación

**Versión del documento:** 1.0 — especificación inicial para ejecución por agente de desarrollo  
**Fecha de elaboración:** 14 de octubre de 2026  
**Estado:** aprobado conceptualmente; implementación pendiente  
**Plataforma inicial:** Windows 10/11 x64  
**Lenguaje del núcleo recomendado:** Rust  
**Prioridad de clientes:** OpenCode → Pi → Hermes  
**Alcance de proveedores inicial:** API oficial de DeepSeek; investigación de compatibilidad y políticas separadas para OpenCode Go  
**Nombre de trabajo:** PeakGuard (verificar disponibilidad del nombre antes de publicar)

> **Instrucción para el agente desarrollador:** Este archivo define el producto, las decisiones aprobadas, las restricciones y el orden de implementación. No confundir características **requeridas**, **condicionadas a viabilidad** y **futuras**. Investigar APIs y versiones instaladas antes de escribir adaptadores. Entregar evidencia verificable para cada tarea. No declarar soporte ni protección global cuando una ruta de llamadas no esté interceptada o comprobada.

---

# 1. Propósito del proyecto

Desarrollar una herramienta **local, ligera, autónoma y configurable** para supervisar y controlar el uso de modelos de IA cuya tarifa cambia por horario, comenzando por DeepSeek. El problema que resuelve es que determinadas peticiones durante las horas **peak** cuestan el doble que fuera de ellas. PeakGuard debe advertir con antelación y, según el modo elegido por el usuario, impedir el consumo durante el periodo caro o detener el **turno activo del agente sin cerrar la aplicación**.

PeakGuard no es un agente de IA adicional. No necesita invocar un LLM para decidir. Es un sistema determinista de políticas horarias y control de solicitudes que se ejecuta en la computadora del usuario.

## 1.1. Objetivo general

Evitar el consumo no deseado en franjas tarifarias caras, preservando una buena experiencia en herramientas de programación asistida por IA como OpenCode, Pi y Hermes.

## 1.2. Objetivos específicos

1. Calcular horarios peak/off-peak con reglas por proveedor y modelo, sin depender de la zona horaria del equipo para tomar decisiones.
2. Identificar qué **proveedor factura** la petición y qué modelo/alias se utiliza; no inferir el precio únicamente por contener `deepseek` en el nombre.
3. Ofrecer tres modos: **bloqueo estricto**, **restringido (Stop nativo)** y **solo alertas**.
4. Notificar a los **30, 10 y 5 minutos** previos y mostrar cuenta regresiva durante los **últimos 10 segundos anteriores** al inicio del pico.
5. Impedir nuevas peticiones cubiertas por la política durante peak, incluso si un agente ignora las notificaciones (siempre que la ruta esté protegida por el gateway).
6. En modo restringido, solicitar la **cancelación del turno** mediante API de la aplicación (`session.abort()`, `ctx.abort()`, `/stop` o equivalente validado), **sin cerrar OpenCode/Pi/Hermes**.
7. Conservar el historial y la sesión en la medida que lo permita el cliente; nunca prometer que una generación abortada seguirá desde el mismo token.
8. Ofrecer indicadores compactos en TUI/sidebar y notificaciones de escritorio/Telegram, sin consumir tokens para avisar.
9. Mantener un núcleo binario Rust y adaptadores pequeños por aplicación; incluir MCP como interfaz **complementaria y opcional**, no como motor de bloqueo.
10. Ser transparente sobre cobertura, fallos y costos estimados; no prometer ahorro exacto ni cobertura de programas que el usuario no haya configurado.

## 1.3. Definiciones esenciales

- **Peak:** intervalo durante el cual el proveedor cobra una tarifa mayor.
- **Off-peak:** intervalo de tarifa reducida según la política del proveedor.
- **Turno activo:** generación del modelo y ciclo de herramientas asociado que un cliente está procesando.
- **Stop nativo / abort:** cancelación de la ejecución del turno en el cliente; **no** equivale a cerrar el proceso, borrar una sesión ni suspender una generación recuperable bit a bit.
- **Gateway local:** servidor HTTP en loopback que recibe solicitudes compatibles y las reenvía al proveedor después de aplicar políticas.
- **Ruta protegida:** ruta de solicitudes verificablemente obligada a pasar por PeakGuard o bloqueada por el control nativo antes de enviarse.
- **Adaptador:** extensión/plugin específico que reporta modelo, proveedor y sesión al núcleo, representa la UI y ejecuta control nativo.
- **MCP:** protocolo para herramientas/recursos consultivos; no proporciona interceptación HTTP ni control de inferencia por sí solo.

---

# 2. Alcance funcional y límites

## 2.1. Incluido en el producto objetivo

- Núcleo local configurable.
- Reglas horarias con prioridad por proveedor y modelo.
- Detección de sesión, cliente, proveedor y modelo cuando las interfaces lo permitan.
- Gateway HTTP de DeepSeek directo.
- Control nativo de sesiones en OpenCode y Pi; estudiar alcance real de Hermes.
- Soporte para múltiples procesos/sesiones y cambios de modelo durante la ejecución.
- UI TUI compacta; sidebar de OpenCode V2 cuando la versión instalada la soporte.
- Alertas locales y Telegram opcional.
- Logs de eventos no sensibles y diagnóstico de cobertura.
- Instalador/desinstalador y documentación Windows; Linux/macOS posteriores.
- MCP de solo consulta inicialmente.

## 2.2. Fuera del MVP

- Interceptar automáticamente y sin configuración **todo** el tráfico HTTPS del sistema.
- Instalar certificados raíz, hacer MITM, modificar hosts o firewall como mecanismo predeterminado.
- Congelar/reanudar exactamente una generación de tokens interrumpida.
- Rehacer automáticamente herramientas que pudieron generar efectos secundarios.
- Reanudación automática de turnos al salir de peak.
- Cambiar automáticamente a otros modelos, salvo fase futura con autorización explícita.
- Calcular la factura exacta de todas las plataformas sin acceso a datos oficiales de uso y deducción.
- Modificar archivos internos de OpenCode, Pi o Hermes mediante parches no soportados.

## 2.3. Principios no negociables

**P1 — No cerrar al agente.** En modo restringido, `abort/stop` cancela el turno; la aplicación sigue abierta. Nunca sustituirlo por `process.kill`, `taskkill`, `ctx.shutdown()` ni detener servicios globales.

**P2 — No apagar otros modelos.** Una sesión que cambie a un modelo no sujeto a la regla debe poder operar normalmente.

**P3 — Cobertura honesta.** Una aplicación con ruta directa sin gateway y sin control nativo comprobado debe verse como **NO PROTEGIDA**, nunca como bloqueada.

**P4 — No nuevos cargos evitables.** El núcleo no utilizará LLM para avisos, ni enviará mensajes de chat que provoquen nuevas llamadas.

**P5 — Sin reanudación implícita.** Al terminar peak se levanta el bloqueo; el usuario decide cuándo enviar un nuevo mensaje para continuar.

**P6 — Sin secretos en logs.** No registrar claves, cuerpos de prompt, contenidos de archivos ni respuestas del modelo.

---

# 3. Horarios, tarifas y fuente de verdad

## 3.1. Horario base confirmado

Para los modelos de DeepSeek cubiertos por la política peak/off-peak, la documentación oficial enumera:

- **Lunes a viernes, 01:00–04:00 UTC**.
- **Lunes a viernes, 06:00–10:00 UTC**.
- El resto del tiempo es off-peak.

En Perú (UTC−5), por conversión de los intervalos anteriores:

| Periodo | Hora de Perú | Día local |
|---|---|---|
| Peak 1 | 20:00–23:00 | Domingo a jueves |
| Peak 2 | 01:00–05:00 | Lunes a viernes |
| Fuera de pico | Resto del calendario | Según corresponda |

**Importante: políticas distintas según el proveedor facturable.** Al revisar las fuentes oficiales, la página de precios de la **API directa de DeepSeek** indica exclusión de **festivos públicos chinos**; la página consultada de **OpenCode Go** publica los intervalos UTC sin establecer esa misma excepción en su texto de horarios. El motor **no debe compartir una exclusión de festivos entre proveedores de forma automática**. Cada perfil lleva sus propias reglas y fecha de verificación. Las condiciones pueden cambiar: comprobar de nuevo antes de fijar los datos de producción.

Fuente DeepSeek: https://api-docs.deepseek.com/quick_start/pricing/  
Fuente OpenCode Go: https://opencode.ai/v2/docs/console/go/

## 3.2. Reglas de precios

- Mantener el precio como **metadato del proveedor**, no como condición codificada rígidamente.
- Al inicio, la prioridad es el horario y el bloqueo; el cálculo de costos es auxiliar.
- Los valores de precios y aliases deben estar versionados y tener `checked_at` y `source_url`.
- No asignar reglas de la API oficial a OpenRouter, OpenCode Go u otro reseller sin comprobar condiciones propias.
- Alias históricos como `deepseek-v4-flash`, `deepseek-v4-pro`, `deepseek-chat`, etc. **no son garantía** de equivalencia tarifaria: resolver modelo real + proveedor y versionar mapeos.
- Los modelos y su ciclo de vida cambian; el proyecto no debe depender de una lista estática en el binario.

## 3.3. Tratamiento de solicitudes que cruzan la frontera horaria

No asumir cómo factura el proveedor una solicitud empezada off-peak que sigue produciendo tokens después del inicio de peak si no hay una regla contractual explícita. Diseñar la política conservadora:

1. Mostrar advertencias previas.
2. Permitir margen configurable para no iniciar turnos largos muy cerca de peak (opcional; deshabilitado inicialmente).
3. Al comenzar peak, en bloqueo estricto cerrar/cancelar conexión cubierta; en restringido ejecutar Stop nativo y cerrar el stream si continúa.
4. Registrar el evento como `interrupted_at_boundary`, **sin afirmar un ahorro exacto**.

## 3.4. Precedencia y actualización

Orden propuesto de prioridad: **excepción manual explícita autorizada > regla específica de proveedor/modelo > regla del proveedor > regla global**. Una excepción manual nunca debe activarse accidentalmente y debe tener caducidad.

El sistema verifica versiones de políticas en un intervalo configurable, no requiere conexión constante y sigue funcionando con la última política validada. Si el horario es ambiguo o la regla está desactualizada, informa al usuario y aplica la preferencia de seguridad configurada.

---

# 4. Los tres modos definitivos

| Comportamiento | Modo 1: Bloqueo estricto | Modo 2: Restringido / Stop nativo | Modo 3: Solo alertas |
|---|---|---|---|
| Avisos 30/10/5 minutos | Sí | Sí | Sí |
| Cuenta regresiva final de 10 s | Sí | Sí | Sí |
| Impedir nuevas llamadas protegidas en peak | Sí | Sí | No |
| Actuación sobre turno activo | Cancelar stream y/o petición en gateway | Solicitar Stop nativo; gateway de respaldo | Ninguna |
| Cerrar la aplicación | Nunca | **Nunca** | Nunca |
| Eliminar historial/conversación | Nunca | **Nunca** | Nunca |
| Reanudar generación desde el mismo token | No | No | No aplica |
| Reanudar trabajo automáticamente al salir de peak | No | **No** | No aplica |
| Actuar sobre modelos no protegidos | No | No | No |

## 4.1. Modo 1 — `strict_block`

**Objetivo:** impedir el tráfico facturable protegido durante peak incluso si el cliente no ofrece Stop nativo.

- Antes de peak: monitorizar y notificar.
- Exactamente al límite: cambiar política a `BLOCKED`, dejar de admitir nuevas solicitudes protegidas, cancelar streams cubiertos activos y cerrar conexiones ascendentes según capacidad de transporte.
- Si el cliente reintenta: responder con error estructurado de política, sin retransmitir al proveedor.
- Mantener abiertas las aplicaciones y preservar el resto del trabajo.
- Al finalizar: permitir peticiones nuevas, sin dispararlas automáticamente.
- En errores de política/reloj/estado del servicio: actuar conforme a `fail_closed` para rutas protegidas configuradas.

## 4.2. Modo 2 — `restricted_stop` (**modo recomendado**)

**Objetivo:** lograr el equivalente del botón **Stop** del agente. **No se congela la ejecución para continuar desde el mismo token.** Se cancela el turno sin apagar OpenCode, Pi o Hermes.

Flujo requerido:

1. Identificar **sesiones activas realmente asociadas** a un proveedor/modelo peak. Si no se puede vincular sesión-proveedor con suficiente confianza, **no abortar indiscriminadamente** otros turnos.
2. Al entrar en peak, bloquear de inmediato **nuevas peticiones cubiertas** desde el gateway.
3. Solicitar Stop al adaptador correspondiente **una sola vez por turno** (idempotencia).
4. Esperar acuse del adaptador dentro de un timeout configurable (por ejemplo, 2 s como punto inicial sujeto a pruebas).
5. Si el Stop no se confirma o el flujo continúa, cancelar también la conexión ascendiente cubierta. Marcar `NATIVE_STOP_FAILED` o `FALLBACK_CANCELLED` según corresponda.
6. Mostrar un mensaje **local no generado por LLM**, informando que el turno terminó por política y que la sesión sigue disponible.
7. Durante peak: bloquear nuevas solicitudes del proveedor/modelo protegido. Permitir cambio manual de modelo y demás funciones no afectadas.
8. Después de peak: quitar restricción. Mostrar `LISTO PARA CONTINUAR`; **no** crear un nuevo turno ni reejecutar herramientas.

**Significado explícito del Stop:**

- `OpenCode session.abort()` cancela una sesión activa; **no** la elimina.
- `Pi ctx.abort()` es un control de interrupción de flujo; **no** usar `ctx.shutdown()`.
- Hermes dispone de `/stop`, pero en determinados contextos interrumpe también trabajo en background asociado; verificar superficie y alcance **antes** de automatizarlo.
- Si el mecanismo nativo no existe, no está confirmado o es demasiado amplio: degradar a bloqueo del gateway + aviso. No matar procesos del sistema.

**Si el modelo está ejecutando herramientas:** lo que ya haya escrito en disco o ejecutado puede permanecer; un Stop puede dejar una operación parcial. PeakGuard **no revertirá archivos**, procesos, commits ni llamadas externas automáticamente.

## 4.3. Modo 3 — `notify_only`

- No intercepta/cancela tráfico por razones de horario.
- Publica avisos de 30/10/5 min, cuenta regresiva de los últimos 10 s, inicio y fin.
- UI/TUI y Telegram opcional.
- Si el usuario continúa, las solicitudes siguen.
- Debe indicarse explícitamente `NO HAY BLOQUEO ACTIVO`.

## 4.4. Comportamiento al cambiar de modo durante peak

- `notify_only → strict_block`: bloqueo inmediato y cancelación de streams cubiertos.
- `notify_only → restricted_stop`: bloqueo inmediato + Stop nativo de turnos cubiertos.
- `strict_block/restricted_stop → notify_only`: desbloqueo por autorización explícita; registrar evento, no reanudar el turno cancelado.
- `restricted_stop → strict_block`: mantener el bloqueo, sin volver a enviar Stops ya aplicados.
- Cambiar de proveedor/modelo a uno no sujeto a peak: no bloquear ese destino; validar que no haya un fallback de proveedor implícito no protegido.

---

# 5. Notificaciones, contador e interfaz

## 5.1. Línea temporal por cada comienzo de peak

| Evento | Acción mínima |
|---|---|
| T−30 min | Aviso discreto + estado del modo + hora de inicio |
| T−10 min | Segundo aviso, indicando cliente/modelo/sesiones cuando se conozcan |
| T−5 min | Advertencia prioritaria y explicación de qué modo se ejecutará |
| T−10 s hasta T−1 s | Cuenta regresiva visible **antes** del comienzo de peak |
| T=0 | Aplicación inmediata de la política; Stop nativo según modo |
| Durante peak | Estado persistente y hora de fin |
| Fin de peak | Desbloqueo, notificación y opción de continuar **manualmente** |

**Reglas:** deduplicar alertas por `(provider, period_start, threshold, channel)`; registrar últimos avisos para no duplicarlos en reinicios; cuando el sistema arranque ya dentro de una ventana, emitir aviso de estado y actuar inmediatamente; si arranca a T−4 min, no reproducir artificialmente alertas de 30/10/5 min ya vencidas. El usuario puede activar/desactivar canales y umbrales.

## 5.2. Diseño objetivo de TUI / sidebar

Panel propuesto:

```text
┌─────────────────────────────────┐
│ PeakGuard         ● OFF-PEAK   │
├─────────────────────────────────┤
│ Cliente: OpenCode V2           │
│ Proveedor: DeepSeek Direct     │
│ Modelo: deepseek-flash         │
│ Cobertura: PROTEGIDO          │
│                               │
│ Próximo peak: 20:00 PET       │
│ Cuenta atrás: 00:29:45        │
│ Modo: RESTRINGIDO / STOP      │
│ Alertas: 30 · 10 · 5 min      │
└─────────────────────────────────┘
```

Estados visuales: `OFF-PEAK`, `PEAK`, `PREPARING`, `STOP_REQUESTED`, `BLOCKED`, `WARNING_ONLY`, `UNCOVERED`, `DEGRADED`, `UNKNOWN_RULE`.

**Requisitos de UX:**

- Preferir `sidebar.content` o `sidebar.footer` en OpenCode V2 si la versión instalada los soporta; fallback a footer/status/toast en otras interfaces.
- Barra lateral compacta, configurable y no intrusiva; no tomar foco ni escribir en el prompt.
- Contador visible por UI, recalculado localmente desde la siguiente transición; evitar enviar SSE cada segundo si no hace falta.
- Mostrar quién controla el coste: `provider` + `model` + `coverage`, no solo el modelo.
- Toda acción de emergencia debe dar mensaje humano legible sin llamar al LLM.
- Si la TUI no permite el sidebar deseado, priorizar la **funcionalidad** sobre un hack visual frágil.

## 5.3. Telegram opcional

- Alta del bot vía token y chat ID autorizados (BotFather + verificación de propiedad).
- Secretos en variables de entorno o almacén de credenciales, no en TOML ni logs.
- Avisos de horarios, inicio/fin del bloqueo y fallos críticos.
- Manejar reintentos moderados, rate limits y pérdida de conexión sin afectar el motor de políticas.
- Comandos `/status` y `/nextpeak` son opcionales en una fase posterior; acciones de cambiar política requieren autenticar al dueño y consentimiento explícito.

---

# 6. Arquitectura técnica

## 6.1. Decisión principal

**Un núcleo en Rust + gateway HTTP + adaptadores por cliente + MCP opcional.** Un solo ejecutable principal en Windows, con plugins pequeños en los lenguajes que obligue cada cliente (TypeScript/JavaScript para OpenCode y Pi, integración específica para Hermes).

```text
                   ┌──────────────────┐
                   │    OpenCode      │──┐
                   ├──────────────────┤  │
                   │    Pi            │──┼─> Adaptadores: modelo, sesión,
                   ├──────────────────┤  │   eventos, Stop nativo, TUI
                   │    Hermes        │──┘
                   └──────────────────┘
                              │
                    API de control local / SSE
                              │
                              ▼
    ┌───────────────────────────────────────────────────┐
    │ peakguard.exe  (Rust, proceso local)              │
    │                                                   │
    │  Policy Engine ─ Clock/Schedule ─ Model Registry  │
    │       │                    │                      │
    │  Session Registry    Notification Engine          │
    │       │                    │                      │
    │  HTTP Gateway        Telegram / Desktop           │
    │       │                                           │
    │  Control API + SSE ─ Storage/Logging ─ MCP opc.   │
    └───────────────────────────────────────────────────┘
                      │
                      ▼
    ┌───────────────────────────────────────────────────┐
    │ Rutas upstream verificadas                        │
    │ - DeepSeek API directa: implementación inicial    │
    │ - OpenCode Go: factibilidad de proxy POR VALIDAR  │
    │ - Otros proveedores: integración futura           │
    └───────────────────────────────────────────────────┘
```

**MCP solo no es suficiente:** el agente puede no invocar una herramienta de PeakGuard y de todos modos enviar peticiones. El control efectivo se implementa en gateway y/o hooks nativos verificables.

## 6.2. Componentes del núcleo

| Módulo | Responsabilidad | Límites |
|---|---|---|
| `clock` | UTC, hora local, transición futura, detección de cambios bruscos de reloj | No fijar horario de Perú como regla universal |
| `schedule` | Ventanas, festivos/excepciones y precedencias | Políticas por proveedor |
| `registry` | Proveedores, modelos, aliases, fechas y fuente | No inferir precio solo por nombre |
| `policy` | Evaluar `ALLOW/BLOCK/STOP_REQUIRED/NOTIFY` y modos | Determinista y testeable |
| `gateway` | HTTP upstream, auth de proveedor, streaming, cancelaciones, errores | No intercepta clientes no configurados |
| `sessions` | Sesiones/turnos identificados por adaptadores y corr. ID | No detener sesiones no relacionadas |
| `control_api` | Estado/configuración/control local autenticado | Separar credencial de control de API de DeepSeek |
| `events` | SSE o IPC de cambios significativos | No usar broadcast por segundo |
| `notifications` | TUI/escritorio/Telegram y deduplicación | Nunca generar texto con LLM |
| `storage` | TOML + estado mínimo persistente + logs redacted | No guardar prompts/respuestas |
| `mcp` | Lecturas consultivas opcionales | Nunca autoridad única de bloqueo |

## 6.3. Tecnologías propuestas (decisión de implementación, no obligación de librería exacta)

- Rust estable, edition actual compatible con dependencias.
- Tokio para tareas asíncronas.
- Axum/Hyper o alternativa Rust HTTP madura y mantenida, con `rustls` para conexiones TLS ascendentes.
- `serde` + `toml` para configuración; validaciones con esquema tipado.
- Hora UTC y IANA TZ para presentar la zona local; pruebas de límites y DST.
- SSE mediante canal de eventos, con rehidratación de estado en reconexión.
- Logs estructurados, rotación local, secretos redactados.
- Estado mínimo en JSON atómico o SQLite liviano si la consistencia concurrente lo exige (decidir tras spike).
- Windows Service o tarea de inicio automático **opcional**, instalada con consentimiento; funcionamiento manual mediante CLI obligatorio.

## 6.4. Topología de red y puertos

- Gateway: `http://127.0.0.1:8765` (ejemplo configurable).
- API de control: preferir socket/IPC o un segundo puerto `127.0.0.1` con token de control restringido; **nunca** publicar sin autenticación en `0.0.0.0`.
- API ascendiente HTTPS del proveedor verificado.
- Separar físicamente o por rutas/autenticación el tráfico del gateway de la API administrativa.
- Validar y restringir upstreams (allowlist); impedir SSRF, open proxy o desvío a destinos arbitrarios.
- No exponer la API administrativa mediante CORS amplio; no confiar únicamente en que está en loopback.

## 6.5. Flujo de una solicitud protegida

```text
Solicitud entrante
  -> autenticar/identificar ruta
  -> leer proveedor facturable + modelo efectivo
  -> clasificar cobertura y ventana actual
  -> evaluar política
      ALLOW: transmitir al upstream
      BLOCK: error de política local; nada llega al upstream
      STOP_REQUIRED: bloquear y solicitar Stop nativo por canal de control
      NOTIFY: transmitir, emitiendo avisos según calendario
  -> streaming de respuesta (si autorizado)
  -> si comienza peak durante stream: aplicar política seleccionada
  -> registrar metadatos mínimos sin cuerpos
```

**HTTP 429/503:** evitar su uso por defecto para bloqueos de política, porque clientes pueden interpretar que deben reintentar. Proponer error estructurado propio (por ejemplo HTTP 403 con `peakguard_policy_blocked`) y comprobar compatibilidad con cada SDK. Un cliente sin soporte puede mostrar error: esto se documenta como comportamiento previsto; preferir el pre-bloqueo nativo si existe.

## 6.6. Identificación segura de sesiones

A cada solicitud se asocia: `request_id`, `provider_id`, `model_id`, `client_id`, `session_id`, `turn_id`, `started_at`, `policy_revision`, `coverage_state`. Los IDs de sesión/turno pueden provenir de adaptador autenticado; no confiar ciegamente en headers arbitrarios. Si faltan, tratar como tráfico sin asociación, aplicar bloqueo de gateway pero **no Stop nativo indiscriminado**.

---

# 7. Integraciones por aplicación

## 7.1. OpenCode — primera integración

**Compatibilidad principal:** OpenCode V2 donde sea viable; detectar versión instalada. OpenCode SDK documenta `session.abort({ path })` y su sistema de plugins permite eventos. Su plugin CLI V2 ofrece slots `sidebar.content` / `sidebar.footer`. Estas interfaces no deben mezclarse sin verificar compatibilidad de paquetes y versión.

**Requisitos:**

- Descubrir modelo seleccionado, proveedor, sesión y actividad del turno.
- Conectar al controlador local con credenciales separadas.
- Ejecutar Stop por **session ID** solo si corresponde a un turno DeepSeek protegido.
- Implementar tablero lateral con temporizador, modo y cobertura.
- Escuchar eventos de inicio/fin de ventana y reportar ACK de Stop.
- Documentar la configuración del proveedor para que peticiones directas atraviesen el gateway.
- Crear matriz de compatibilidad v1/v2 y no instalar plugin incorrecto.

**OpenCode Go:** es un **servicio y ruta de facturación diferenciados**. La existencia de un plugin o de `session.abort()` **no prueba** que se pueda configurar Go para reenviar toda la inferencia al proxy. Se requiere spike y pruebas de red; hasta entonces mostrar `NATIVE_ONLY` o `UNVERIFIED`, según sea el caso, y no prometer bloqueo absoluto de nuevas peticiones.

## 7.2. Pi — segunda integración

- Extensión ligera TypeScript, eventos del agente y UI mediante `ctx.ui.setStatus`, `ctx.ui.setWidget`, `ctx.ui.notify` (comprobar versión).
- `ctx.abort()` para interrumpir el turno actual; **no** usar `ctx.shutdown()`.
- Investigar provider personalizado/gateway para todas las rutas relevantes.
- Asociar Stop a turno de Pi y solo modelo protegido.
- Mantener experiencia visual compacta según capacidades reales de Pi; no asumir sidebar derecha como la de OpenCode.

## 7.3. Hermes — tercera integración

- Investigar plugins, comandos, hooks, TUI y API de gateway para detener **turnos específicos**.
- Documentación de `/stop`: interrumpe agente y también determinados procesos/delegaciones en segundo plano asociados. Puede tener impacto más amplio que el turno de inferencia.
- No automatizar el envío textual de `/stop` sin una API/control validado y prueba de alcance.
- Si no se encuentra Stop seguro de alcance apropiado, dejar solo **bloqueo de gateway + aviso** (sin matar Hermes ni procesos globales).
- Evaluar diferencias CLI/TUI, mensajería, subagentes y tareas en background.

## 7.4. Matriz de capacidad obligatoria

Para cada versión probada de cada cliente almacenar:

| Capacidad | OpenCode | Pi | Hermes |
|---|---|---|---|
| Detectar proveedor | Confirmar en spike | Confirmar en spike | Confirmar en spike |
| Detectar modelo efectivo | Confirmar | Confirmar | Confirmar |
| Asociar sesión a petición | Confirmar | Confirmar | Confirmar |
| Detener turno sin cerrar app | SDK `session.abort` | `ctx.abort` | `/stop` sujeto a alcance |
| Requerir gateway | Sí, para garantía de bloqueo HTTP | Sí, para garantía de bloqueo HTTP | Sí, para garantía de bloqueo HTTP |
| Sidebar derecha | V2 ofrece slots | UI disponible, diseño distinto | Investigar |
| Alertas nativas | Sí | Sí | Investigar |
| Trabajo en segundo plano | Investigar | Investigar | Atención especial |

Estados de compatibilidad permitidos: `VERIFIED`, `PARTIAL`, `UNSUPPORTED`, `UNKNOWN` con `version`, `tested_at`, `evidence`.

---

# 8. Máquina de estados y políticas de fallo

## 8.1. Estados

```text
DISABLED ───────────────> OFF_PEAK
                           │
                           ▼
                     WARNING_30 / 10 / 5
                           │
                           ▼
                     COUNTDOWN_10S
                           │
                           ▼
                        PEAK_ACTIVE
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
            BLOCKED     STOPPING     WATCHING
                           │
                   STOPPED / DEGRADED
              └────────────┼────────────┘
                           ▼
                       OFF_PEAK
```

Distinguir estado horario, política y estado de sesión: no representarlos como una sola enumeración con cientos de combinaciones. La política se recalcula a partir de la fuente de verdad UTC.

## 8.2. Garantías

- **Bloqueo antes del Stop:** el motor impide nuevas solicitudes cuando llega T=0; después solicita Stop, para evitar una ventana de fuga mientras espera ACK.
- **Idempotencia:** enviar una única acción Stop por `(client, session, turn, transition)`.
- **Concurrencia:** múltiples clientes y streams no deben compartir incorrectamente el mismo `session_id`.
- **Reinicio:** recuperar ventana horaria, deduplicación y sesiones conocidas con estado de cobertura; no recrear ni reanudar turnos.
- **Cambio de reloj:** detectar saltos/suspensión; revaluar inmediatamente al reanudar Windows.
- **Crash del gateway:** en rutas configuradas exclusivamente por gateway, la conexión falla; nunca autorizar bypass directo automático silencioso.
- **Control API caída:** el gateway sigue aplicando la última política segura; adaptador indica que el estado es degradado.
- **Red caída:** reglas cached siguen disponibles; Telegram falla de forma aislada.

## 8.3. Fallos previsibles y tratamiento

| Caso | Acción |
|---|---|
| Modelo desconocido | No asumir DeepSeek por fragmentos de texto; identificar/avisar como desconocido |
| Proveedor sin reglas verificadas | Seguir política explícita `unknown_rule`, registrar y avisar |
| Solicitud directa sin gateway | Marcar `UNCOVERED`; sugerir configuración del cliente |
| Stop no disponible | Bloquear gateway y notificar degradación |
| Stop no confirmó | Timeout y cierre stream cubierto; no terminar el proceso |
| Proxy no responde | Cliente falla cerrado cuando se configuró protección estricta |
| Respuesta SSE a mitad de token | Cancelar upstream y downstream sin duplicar solicitudes |
| Cliente reintenta un bloqueo | Gateway reevalúa y rechaza sin reenviar; rate-limit de logs |
| Usuario cambia a modelo no restringido | Permitir ese modelo si proveedor y ruta están verificados |
| Tarea escribe archivos | No revertir; advertir que el turno puede quedar parcial |

---

# 9. Contratos propuestos de configuración y comunicación

> Los ejemplos siguientes definen contratos **a implementar**, no APIs que ya existan. Versionar su esquema (`config_version`, `api_version`). Validar formatos antes de habilitar peticiones reales.

## 9.1. Ejemplo `peakguard.toml`

```toml
config_version = 1

[general]
enabled = true
mode = "restricted_stop"  # strict_block | restricted_stop | notify_only
show_timezone = "America/Lima"
start_on_login = false

[alerts]
minutes_before = [30, 10, 5]
final_countdown_seconds = 10
desktop = true
tui = true
telegram = false
notify_on_peak_end = true

[gateway]
listen_address = "127.0.0.1"
listen_port = 8765
allow_remote_connections = false
fail_closed_for_protected_routes = true
active_stream_on_peak = "cancel"

[control]
listen_address = "127.0.0.1"
listen_port = 8766
token_env = "PEAKGUARD_CONTROL_TOKEN"

[stop]
ack_timeout_ms = 2000
auto_resume = false
fallback_to_gateway_cancel = true

[telegram]
bot_token_env = "PEAKGUARD_BOT_TOKEN"
chat_id_env = "PEAKGUARD_CHAT_ID"

[[providers]]
id = "deepseek_direct"
upstream = "https://api.deepseek.com"
enabled = true
schedule_id = "deepseek_direct_verified"

[[providers]]
id = "opencode_go"
enabled = false
schedule_id = "opencode_go_verified"
# NO configurar upstream hasta verificar routing, credenciales y cobertura.

[[schedules]]
id = "deepseek_direct_verified"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["01:00-04:00", "06:00-10:00"]
exclude_chinese_public_holidays = true
source_url = "https://api-docs.deepseek.com/quick_start/pricing/"
# source_checked_at requiere fecha de verificación durante instalación.

[[schedules]]
id = "opencode_go_verified"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["01:00-04:00", "06:00-10:00"]
exclude_chinese_public_holidays = false
source_url = "https://opencode.ai/v2/docs/console/go/"
```

**Nota:** la excepción de festivos de la API directa y su fuente deben revalidarse durante T01. Los horarios de Go no demuestran que su tráfico pueda dirigirse al proxy; `enabled = false` hasta confirmar la integración.

## 9.2. Control API local, propuesta

| Método | Ruta propuesta | Uso | Seguridad |
|---|---|---|---|
| GET | `/api/v1/health` | Disponibilidad mínima | Local, no secretos |
| GET | `/api/v1/status` | Estado, próximo cambio, modo, cobertura | Token local |
| GET | `/api/v1/sessions` | Sesiones conocidas y estado | Token local |
| GET | `/api/v1/events` | SSE de transiciones | Token local, reconexión |
| PUT | `/api/v1/config/mode` | Cambiar modo | Token + validación |
| POST | `/api/v1/adapters/register` | Alta autenticada de adaptador | Token por instalación/proceso |
| POST | `/api/v1/adapters/stop-ack` | Confirmación de Stop | Asociar turno y petición |
| POST | `/api/v1/policies/reload` | Recargar reglas validadas | Solo administración |

Respuesta de bloqueo propuesta:

```json
{
  "error": {
    "code": "peakguard_policy_blocked",
    "message": "Petición restringida durante horario peak.",
    "provider_id": "deepseek_direct",
    "model_id": "deepseek-flash",
    "mode": "restricted_stop",
    "next_off_peak_utc": "<ISO-8601-UTC>",
    "retry_automatically": false
  }
}
```

`<ISO-8601-UTC>` es marcador documental; el valor en producción será un timestamp UTC válido. No exponer claves o IDs sensibles de sesión en el cuerpo del error.

## 9.3. API Gateway (primera compatibilidad)

- Soportar los endpoints compatibles con la **API directa que el cliente realmente use** (investigar OpenAI Chat Completions y Responses en la versión instalada).
- Streaming SSE sin cargar el cuerpo completo en memoria ni acumular tokens.
- Preservar headers necesarios y filtrar hop-by-hop; jamás loguear `Authorization`.
- Manejar timeouts, desconexiones, cancelación de upstream y límites de tamaño.
- `provider_id` no debe poder establecerlo una llamada arbitraria para saltarse el control.
- No añadir modelos desconocidos o endpoints de terceros a una allowlist sin revisión.

## 9.4. MCP opcional

Solo lectura en versión inicial:

- `peakguard_status`
- `peakguard_next_peak`
- `peakguard_models`
- `peakguard_coverage`

Cambios como `set_mode` deben estar deshabilitados por defecto y, si se implementan, sujetos a consentimiento interactivo; no permitir que el LLM se autodesbloquee durante peak mediante una tool.

---

# 10. Seguridad, privacidad, rendimiento y distribución

## 10.1. Seguridad y privacidad

- Mínimos privilegios; no administrador para uso cotidiano.
- Token de control generado al instalar, con permisos de lectura restringidos al usuario.
- API keys en variables de entorno o almacén seguro del sistema; **nunca** introducir tokens reales en ejemplos y commits.
- Evitar CORS permisivo, DNS rebinding y peticiones desde navegadores al plano de control.
- Upstreams permitidos, certificados TLS verificados, sin MITM.
- Redactar logs, habilitar diagnóstico detallado solo por acción explícita y sin volcar prompts.
- Sanitizar eventos de adaptadores; no confiar en IDs de sesión remotos sin vínculo autenticado.
- No aceptar comandos globales destructivos desde MCP o Telegram.
- Desinstalación revierte solo los cambios realizados por PeakGuard, preservando otras configuraciones del usuario.

## 10.2. Objetivos de rendimiento (a medir, no garantías preexistentes)

- El núcleo inactivo debe usar CPU mínima (objetivo operacional: por debajo del 1 % en medición sostenida del equipo de prueba).
- Objetivo inicial de memoria inactiva: **≤40 MiB RSS** en build release Windows, sujeto a medición y corrección si resulta irrealista.
- Objetivo inicial de latencia añadida por gateway local: **p95 ≤25 ms** sin contar latencia upstream, bajo carga de prueba local.
- No usar polling a 1 s en cada adaptador si se puede calcular contador local con timestamp UTC y sincronizar por eventos.
- Streaming debe manejar múltiples sesiones concurrentes con backpressure y cancelación estable.
- Registrar tamaño final del binario por plataforma y buscar optimización solo después de medir.

## 10.3. Distribución

**MVP:** `peakguard.exe`, archivo de ejemplo, comando de instalación de integración y documentación.  
**Versión posterior:** instalador Windows firmado si es viable, inicio automático opcional, desinstalador, builds reproducibles y artefactos con hash SHA-256.  
**Futuro:** Linux y macOS, pruebas específicas de servicios y notificaciones.

---

# 11. Estructura de repositorio sugerida

```text
peakguard/
├── README.md
├── LICENSE                         # elegir licencia explícitamente
├── CHANGELOG.md
├── docs/
│   ├── SPEC.md                     # esta especificación
│   ├── ARCHITECTURE.md
│   ├── DECISIONS.md                 # ADRs
│   ├── COMPATIBILITY.md
│   ├── SECURITY.md
│   ├── USER_GUIDE.md
│   └── TEST_PLAN.md
├── core/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   ├── clock/
│   │   ├── schedule/
│   │   ├── registry/
│   │   ├── policy/
│   │   ├── gateway/
│   │   ├── sessions/
│   │   ├── control_api/
│   │   ├── events/
│   │   ├── notifications/
│   │   ├── storage/
│   │   └── mcp/
│   └── tests/
├── adapters/
│   ├── opencode/
│   ├── pi/
│   └── hermes/
├── config/
│   └── peakguard.example.toml
├── test-fixtures/
│   ├── mocked-provider/
│   ├── schedules/
│   └── streams/
├── scripts/
│   ├── build-windows.ps1
│   └── smoke-test.ps1
└── .github/workflows/               # si se usa GitHub
```

La división interna puede cambiar si el agente justifica una alternativa con un ADR. Evitar crear carpetas sin código ni pruebas solo por imitar esta propuesta.

---

# 12. Plan detallado de desarrollo: tareas y entregables

**Regla de ejecución:** desarrollar en orden de dependencias, crear commits por tarea, probar con proveedor simulado antes de utilizar API real y no dar por terminada una etapa porque el código simplemente compile. Cada tarea tiene un entregable y un criterio de aceptación. Los identificadores T00–T15 serán el backlog oficial.

## Fase A — Investigación y contratos (bloqueante)

### T00 — Auditoría de entorno, fuentes y viabilidad

**Depende de:** nada. **Prioridad:** crítica.

- [ ] Inventariar Windows, versión de Rust y versiones instaladas de OpenCode, Pi y Hermes.
- [ ] Verificar URL, modelo facturable, endpoint, ventana horaria y excepción de festivos **por proveedor**.
- [ ] Determinar si OpenCode Go permite configurar un endpoint base/gateway o si solo tiene control nativo por plugin.
- [ ] Verificar en la versión real los mecanismos `session.abort`, `ctx.abort`, Hermes `/stop` y sus efectos sobre subagentes.
- [ ] Probar con un turno corto si abort cancela y mantiene historial sin cerrar la app.
- [ ] Identificar los endpoints de Chat Completions/Responses realmente usados por la instalación.
- [ ] Registrar incertidumbres, fechas de consulta y pruebas reproducibles.

**Entregables:** `docs/COMPATIBILITY.md`, `docs/DECISIONS.md` (ADR-001), matriz de capacidades por versión, tabla de proveedores/modelos.  
**Aceptación:** cada afirmación de interoperabilidad tiene enlace a fuente oficial **o** experimento reproducible; Go sin gateway probado figura como `UNVERIFIED`.

### T01 — Especificación ejecutable y pruebas de horarios

**Depende de:** T00.

- [ ] Construir fixtures de peak por UTC, cruces de fecha, fin de semana y festivos por perfil.
- [ ] Definir semántica de intervalos **[inicio, fin)**; en instante exacto de inicio peak es peak, en instante de fin es off-peak.
- [ ] Definir test clock inyectable y fuente temporal independiente de UI.
- [ ] Probar Perú UTC−5 y otra zona con DST; la política no cambia con la visualización.
- [ ] Implementar deduplicación de notificaciones y política de datos desactualizados.

**Entregables:** especificación de fechas/tiempos, fixtures y conjunto de tests de frontera.  
**Aceptación:** todos los ejemplos de 3.1 y fronteras pasan; el motor identifica la siguiente transición sin desfases de día.

### T02 — Contratos, amenazas y prototipo de interfaz local

**Depende de:** T00, T01.

- [ ] Fijar `config_version`, `api_version`, claves y campos de eventos.
- [ ] Definir estados de cobertura y políticas `ALLOW/BLOCK/STOP_REQUIRED`.
- [ ] Diseñar autenticación de control y registro de adaptadores.
- [ ] Definir límites de seguridad, estructura de errores y respuesta ante reinicio.
- [ ] Documentar manejo de sesiones no identificadas y scope de Stop.

**Entregables:** `docs/ARCHITECTURE.md`, `docs/SECURITY.md`, esquema inicial TOML/JSON, ADR-002.  
**Aceptación:** existen contratos verificables que permiten comenzar núcleo y adaptador sin supuestos contradictorios.

**Puerta de salida A:** no iniciar implementación real de OpenCode Go ni automatizar `/stop` de Hermes sin resolver el alcance en T00.

## Fase B — Núcleo local y políticas

### T03 — Proyecto Rust, CLI y configuración

**Depende de:** T02.

- [ ] Crear proyecto compilable en `core/`.
- [ ] Implementar `peakguard version`, `status`, `doctor`, `config validate`, `run` y `help`.
- [ ] Leer TOML, validar errores sin panic y ofrecer valores predeterminados seguros.
- [ ] Crear logs redactados, apagado ordenado y manejadores de señales.
- [ ] Preparar build release Windows y CI de `fmt`, `clippy`, `test`.

**Entregables:** binario inicial Windows, `config/peakguard.example.toml`, README de compilación.  
**Aceptación:** compila en máquina limpia con prerequisitos documentados; `config validate` detecta horarios/puertos mal definidos.

### T04 — Motor de horarios y registro de proveedores

**Depende de:** T03 y fixtures T01.

- [ ] Implementar intervalos UTC, prioridad de reglas, excepciones de festivos y caches verificadas.
- [ ] Resolver `current_window`, `next_peak`, `next_off_peak`, `remaining_seconds`.
- [ ] Implementar registry con proveedor, modelo/aliases y status de cobertura.
- [ ] Incluir simulación de reloj para pruebas aceleradas.
- [ ] Revaluación en suspensión/despertar y saltos de hora.

**Entregables:** módulo horario, registro versionado y tests unitarios/property-based.  
**Aceptación:** 100 % de los fixtures críticos pasa; no aparecen falsos peak por conversión horaria.

### T05 — Motor de políticas y tres modos

**Depende de:** T04.

- [ ] Implementar máquina de decisiones para los tres modos.
- [ ] Permitir cambios de modo en tiempo real con validación.
- [ ] Implementar enforcement por proveedor, modelo y política.
- [ ] No afectar modelos no incluidos y registrar decisiones sin prompts.
- [ ] Soportar Stop solicitado, timeout, ACK y fallback controlado.
- [ ] Probar idempotencia con múltiples eventos concurrentes.

**Entregables:** módulo `policy`, tests de tabla de verdad/modos, contrato Stop.  
**Aceptación:** cada caso de la matriz en §4 y fallos de §8 produce exactamente la acción esperada.

## Fase C — Gateway verificable

### T06 — Gateway HTTP para DeepSeek directo (sin streaming primero)

**Depende de:** T05.

- [ ] Servidor solo loopback y allowlist de upstreams.
- [ ] Proxy de endpoints utilizados por cliente de prueba (sin asumir formatos no usados).
- [ ] Preservar request/response, autorización hacia upstream y errores con datos redactados.
- [ ] Evaluar política **antes** de conexión con proveedor.
- [ ] Retornar error de política sin retransmitir si está bloqueado.
- [ ] Test contra proveedor simulado; cero solicitudes ascendentes en modo bloqueo.

**Entregables:** gateway funcional HTTP no-streaming, mock server y prueba end-to-end.  
**Aceptación:** `ALLOW` transmite y `BLOCK` nunca alcanza al mock upstream; tráfico a destinos no permitidos se rechaza.

### T07 — Streaming SSE, concurrencia y cancelación

**Depende de:** T06.

- [ ] Retransmitir streams sin acumular todas las respuestas.
- [ ] Cancelar upstream al cruzar peak o al desconectar el cliente.
- [ ] Soportar múltiples sesiones y streams paralelos con IDs diferenciados.
- [ ] Gestionar cancelación limpia y ausencia de envíos dobles.
- [ ] Validar estrategia de errores 403 con clientes reales y evitar reintentos peligrosos.
- [ ] Benchmark de latencia/memoria en escenarios normales.

**Entregables:** streaming end-to-end, tests de cancelación y carga.  
**Aceptación:** prueba de 20 streams simulados: ningún stream protegido continúa transmitiendo upstream después del corte configurado; no hay fuga de conexiones al terminar.

**Puerta de salida C / MVP de protección:** ya existe bloqueo confiable **solo para rutas realmente configuradas por el gateway**. No anunciar soporte global.

## Fase D — Adaptador OpenCode y UX inicial

### T08 — OpenCode: detección, Stop nativo y cobertura

**Depende de:** T07, investigación T00.

- [ ] Crear plugin compatible con la versión OpenCode seleccionada.
- [ ] Vincular proveedor/modelo, sesión y turno con el núcleo.
- [ ] Validar `session.abort()` contra un turno de prueba; debe cancelarse sin cerrar app.
- [ ] Ejecutar Stop solo cuando está confirmado que el turno activo usa el modelo protegido.
- [ ] Enviar ACK, detectar timeout y degradación.
- [ ] Detectar cambio de modelo, sesiones paralelas, subagentes y rutas no capturadas.
- [ ] Documentar la integración real con API directa.

**Entregables:** adaptador OpenCode, guía de instalación, demo en vídeo/GIF opcional y suite de integración.  
**Aceptación:** en modo restringido se interrumpe el turno cubierto, la aplicación queda abierta, historial accesible y una sesión con otro modelo no es abortada.

### T09 — Sidebar/indicador OpenCode y eventos

**Depende de:** T08.

- [ ] Implementar slots/sidebar en OpenCode V2 comprobado.
- [ ] Mostrar proveedor/modelo, próxima franja, contador, modo y cobertura.
- [ ] Conectar estado y cambios vía SSE/IPC.
- [ ] Alertas 30/10/5 y countdown 10 s sin crear mensajes LLM.
- [ ] Fallback a footer/toast cuando sidebar no exista.
- [ ] Validar terminal estrecha, reconexión y app sin red externa.

**Entregables:** UI integrada y capturas de estados.  
**Aceptación:** indicador legible, las notificaciones no duplican ni invocan al modelo, al reiniciar muestra la ventana correcta.

### T10 — Notificaciones de escritorio, Telegram y deduplicación

**Depende de:** T09 (Telegram podría comenzar tras T05).

- [ ] Crear canal de eventos desacoplado.
- [ ] Notificaciones Windows sin depender de que la TUI esté enfocada.
- [ ] Bot Telegram opcional con gestión segura de token/chat.
- [ ] Evitar spam, reintentos excesivos y fugas de credenciales.
- [ ] Notificar fin del pico, Stop exitoso, Stop fallido, ruta sin protección.

**Entregables:** módulo notificaciones, setup de Telegram, tests sin red real usando mock.  
**Aceptación:** secuencia única 30/10/5 y T=0 por ventana; caída de Telegram no impide bloquear.

**Puerta de salida D / MVP usuario:** protección de API directa + OpenCode + UI + avisos, con cobertura explícita.

## Fase E — Compatibilidad ampliada

### T11 — Estudio e integración OpenCode Go

**Depende de:** T00 y T08.

- [ ] Determinar si Go permite routing de llamadas mediante gateway sin romper autenticación/suscripción.
- [ ] Si sí: configurar gateway compatible y verificar tráfico y modelo efectivo.
- [ ] Si no: ofrecer protección nativa antes de invocar al proveedor, aclarando limitaciones de garantías sobre subagentes/llamadas no interceptadas.
- [ ] Aplicar ventana y reglas **propias de Go**, separadas de DeepSeek directo.
- [ ] Probar límites/reintentos y posibles fallbacks de Go.
- [ ] Mostrar `VERIFIED`, `PARTIAL` o `UNSUPPORTED` según evidencia.

**Entregables:** informe `docs/OPENCODE_GO.md`, implementación condicionada a factibilidad, pruebas.  
**Aceptación:** no se presenta como protegida una ruta que no haya podido observarse o detenerse con garantías.

### T12 — Adaptador Pi

**Depende de:** T09, matriz T00.

- [ ] Extensión Pi para detectar modelo y actividad.
- [ ] `ctx.abort()` en modo restringido sobre turno cubierto.
- [ ] `ctx.ui` para indicador y avisos.
- [ ] Gateway configurable para proveedor directo cuando sea posible.
- [ ] Test de sesión conservada, modelos no afectados y concurrencia.

**Entregables:** plugin Pi, README, matriz de pruebas por versión.  
**Aceptación:** parada de turno sin shutdown, políticas consistentes con OpenCode para tráfico cubierto.

### T13 — Adaptador Hermes

**Depende de:** T09 y hallazgos T00.

- [ ] Investigar alcance de `/stop` sobre turno, herramientas y subagentes.
- [ ] Preferir API de interrupción de sesión con alcance explícito si existe.
- [ ] Nunca invocar un comando de cierre de procesos/daemon como sustituto de Stop.
- [ ] Si Stop tiene alcance demasiado amplio, usar fallback gateway + aviso y documentar degradación.
- [ ] Integrar indicador en UI disponible sin modificaciones intrusivas.
- [ ] Pruebas con turno normal y delegaciones/background.

**Entregables:** adaptador Hermes o informe de limitación + integración parcial, pruebas.  
**Aceptación:** no se interrumpen procesos ajenos de forma inesperada; no se cierra Hermes.

## Fase F — Producto, endurecimiento y distribución

### T14 — Seguridad, persistencia, diagnóstico y empaquetado

**Depende de:** T10–T13, se puede adelantar seguridad.

- [ ] Revisar autenticación, claves y exposición loopback.
- [ ] Probar apagado/reinicio, suspensión/resume de Windows y reloj cambiado.
- [ ] Diseñar persistencia mínima de alertas y estado sin reanudar tareas.
- [ ] Añadir `peakguard doctor` con diagnóstico de cobertura por cliente.
- [ ] Test de fuzz para parser HTTP/config si es factible.
- [ ] Build release, empaquetado, hashes y desinstalación reversible.
- [ ] Medir memoria, latencia y consumo CPU según §10.2.

**Entregables:** `peakguard.exe` release, informe de seguridad, resultados de benchmarks, manual de usuario.  
**Aceptación:** no quedan secretos en logs, no hay API admin expuesta, y `doctor` distingue rutas verificadas/no verificadas.

### T15 — MCP opcional y mejoras posteriores

**Depende de:** T14.

- [ ] MCP local consultivo mediante stdio cuando aporte valor real.
- [ ] `status`, `next_peak`, `coverage`, `models` sin llamadas a LLM.
- [ ] Documentar instalación/desinstalación de MCP sin afectar el core.
- [ ] Diseñar, **no activar por defecto**, comandos administrativos desde MCP/Telegram.
- [ ] Backlog futuro: estimador de ahorro, límites de presupuesto, perfiles por proyecto, sustitución voluntaria de modelo, Linux/macOS.

**Entregables:** servidor MCP opcional, tests de capacidades y permisos, backlog 1.1+.  
**Aceptación:** al deshabilitar MCP, el bloqueo y las notificaciones mantienen toda su funcionalidad.

---

# 13. Dependencias, hitos y Definition of Done

## 13.1. Orden sugerido

```text
T00 ─┬─> T01 ─> T02 ─> T03 ─> T04 ─> T05 ─> T06 ─> T07
     │                                            │
     └──────── investigación de clientes ────────┘
                                                  │
                                                  ▼
                                         T08 ─> T09 ─> T10
                                           │      │
                                           ▼      ├─> T12 (Pi)
                                         T11 (Go) └─> T13 (Hermes)
                                            \       /
                                             ▼     ▼
                                               T14 ─> T15
```

**H0 — Aprobación técnica:** T00–T02.  
**H1 — Núcleo funcional:** T03–T05.  
**H2 — Bloqueo real DeepSeek directo:** T06–T07.  
**H3 — MVP utilizable en OpenCode:** T08–T10.  
**H4 — Soporte ampliado y validado:** T11–T13.  
**H5 — Release y MCP opcional:** T14–T15.

## 13.2. DoD global de cada tarea

Una tarea solo se marca `DONE` si:

1. Tiene implementación o documento correspondiente en el repositorio.
2. Tiene pruebas automatizadas o procedimiento manual reproducible con resultados.
3. No rompe tests ya aprobados ni `cargo fmt/clippy/test`.
4. Actualiza `README`/`CHANGELOG`/compatibilidad cuando corresponde.
5. No agrega secretos ni datos de prompts a los logs.
6. Produce un **resumen de entrega** con archivos cambiados, decisiones, validación, limitaciones y próxima tarea.
7. Cumple todas las condiciones de aceptación de la tarea.

Estados del backlog: `TODO`, `IN_PROGRESS`, `BLOCKED`, `REVIEW`, `DONE`.

## 13.3. Evidencia mínima de una entrega

Formato para el agente:

```markdown
### Entrega TXX — <título>
Estado: DONE / BLOCKED
Resumen: ...
Archivos modificados: ...
Comandos y resultados de pruebas: ...
Compatibilidad verificada: ...
Riesgos o límites: ...
Criterios de aceptación: [x] ...
Commit / revisión: ...
Siguiente tarea recomendada: TYY
```

No marcar `DONE` si únicamente se generó código sin ejecutarlo o inspeccionarlo.

---

# 14. Plan de pruebas de aceptación integral

| ID | Caso | Resultado exigido |
|---|---|---|
| AT-01 | Un minuto antes de peak, modo aviso | Solo cuenta atrás/alerta, tráfico permitido |
| AT-02 | T−30, T−10, T−5 min | Un aviso por umbral/canal/ventana |
| AT-03 | T−10 s | Contador empieza **antes** de peak |
| AT-04 | Llegada a T=0, modo estricto | No sale ninguna petición protegida nueva |
| AT-05 | Stream en curso, modo estricto | Cancelación upstream y estado local actualizado |
| AT-06 | Turno OpenCode + modo restringido | `session.abort` canceló turno, aplicación abierta, historial accesible |
| AT-07 | Turno Pi + modo restringido | `ctx.abort` canceló turno, aplicación abierta |
| AT-08 | Hermes con delegaciones activas | Sin interrupción indebida; fallback si no es seguro |
| AT-09 | Modelo no DeepSeek concurrente | No detenido ni bloqueado por política DeepSeek |
| AT-10 | Cambio de modelo durante peak | Bloqueo se aplica al proveedor/modelo efectivo, no al nombre histórico de la sesión |
| AT-11 | Solicitud directa que no pasa por gateway | Estado `UNCOVERED`; no falsa garantía |
| AT-12 | API Gateway caída | Sin bypass silencioso de rutas protegidas |
| AT-13 | Reinicio durante peak | Políticas se restauran de inmediato; sin nueva ejecución automática |
| AT-14 | Al finalizar peak | Permite nuevas peticiones; **no** reanuda el turno solo |
| AT-15 | Telegram sin conexión | Bloqueo sigue funcionando |
| AT-16 | Festivo chino DeepSeek directo | Aplicar regla oficial del perfil directo verificada |
| AT-17 | Festivo chino OpenCode Go | Aplicar exclusivamente su perfil independiente |
| AT-18 | Hora local distinta o DST | La decisión UTC no cambia |
| AT-19 | Error de política HTTP | No hay reintento infinito ni cobro vía gateway |
| AT-20 | Instalación/desinstalación | No quedan modificaciones no solicitadas ni claves expuestas |
| AT-21 | 20 streams simultáneos | No hay fuga de memoria/conexiones ni Stop cruzado de sesiones |
| AT-22 | MCP desconectado | Gateway y avisos siguen funcionando |

**Nota sobre pruebas de proveedores reales:** minimizar gasto de API; usar mocks deterministas y pruebas reales de bajo consumo únicamente para validar compatibilidad. Registrar diferencias entre resultado observado y esperado.

---

# 15. Riesgos, decisiones pendientes y prioridades

| Riesgo | Impacto | Mitigación |
|---|---|---|
| OpenCode Go no permite custom gateway | Alto | Spike T00; soporte parcial por Stop y cobertura explícita |
| API de plugins cambia entre versiones | Alto | Versionar adaptadores; pruebas CI por versión |
| `abort` no cancela herramienta no cooperativa | Alto | Alcance documentado; no matar proceso global; fallback solo de gateway |
| Stop Hermes interrumpe background | Alto | Investigar scope antes de automatizar |
| Solicitudes no asociadas a sesión | Medio | Bloquear en gateway, no emitir Stop indiscriminado |
| Diferencias de festivos/tarifas | Alto | Perfiles independientes, fuentes y revisión de reglas |
| Reintentos automáticos de SDK | Medio | Error de política probado por cliente y bloqueo antes de upstream |
| Antivirus o políticas de Windows | Medio | Binario limpio, documentación, firma cuando se pueda |
| Costos cruzando transición | Medio | Cancelación en límite y advertencias; no prometer ahorro exacto |
| Bugs de UI que ocultan estado | Medio | Gateway independiente de UI y diagnóstico `doctor` |

Decisiones aún abiertas que el agente debe resolver con evidencia, no con suposiciones: versión exacta de OpenCode, compatibilidad routing de Go, interfaz segura de Stop Hermes, almacenamiento de estado mínimo y librería de notificaciones Windows.

---

# 16. Instrucciones operativas para el agente que ejecutará el proyecto

1. **Lee íntegramente** este archivo y crea el backlog T00–T15 en el repositorio.
2. **No empieces codificando el proxy**: comienza por T00 y confirma rutas, versiones y precios.
3. No pidas confirmaciones triviales: resuelve decisiones técnicas reversibles con ADR breve. Escala solamente cambios que alteren un requisito esencial de esta especificación.
4. Prioriza **Windows + DeepSeek directo + OpenCode**; Pi, Hermes y Go siguen en el backlog según viabilidad.
5. Mantén compilaciones y pruebas funcionando después de cada etapa; realiza commits atómicos.
6. Cuando existan incompatibilidades oficiales, usa fallback documentado y reporta honestamente el nivel de cobertura.
7. Nunca confundir `abort/stop` con `shutdown/kill` ni programar reanudación automática en modo restringido.
8. Antes de probar bloqueos con costo real, habilita simulación de hora (`test_clock`) y upstream local falso.
9. Al concluir una tarea, entrega resumen, tests, archivos y estado. Comienza la siguiente **solo en el mismo ciclo de ejecución autorizado por el usuario**; no simules trabajo asíncrono.
10. El objetivo no es solo que funcione en una demo: debe ser mantenible, extensible y seguro para múltiples sesiones.

## Criterios globales para declarar el MVP listo

- [ ] El binario funciona en Windows y los tests horarios pasan.
- [ ] DeepSeek directo pasa por gateway configurado, streaming incluido.
- [ ] Durante peak, ningún tráfico cubierto sale en modo estricto/restringido.
- [ ] En modo restringido, el cliente OpenCode detiene un turno mediante API nativa **sin cerrar el programa**.
- [ ] En modo solo aviso, no existe bloqueo accidental.
- [ ] Alertas 30/10/5 y 10 segundos finales funcionan y se deduplican.
- [ ] Panel TUI muestra proveedor, modelo, horario, modo y cobertura real.
- [ ] Las rutas no protegidas aparecen explícitamente como tales.
- [ ] La aplicación no guarda secretos ni prompts en logs.
- [ ] Al salir de peak, se desbloquea sin reanudar el turno automáticamente.
- [ ] README incluye instalación, configuración, diagnóstico y desinstalación.

---

# 17. Mejoras de versiones futuras (no bloquear el MVP)

1. Estimador de costo y ahorro por tokens realmente reportados; distinguir estimación de factura.
2. Límites presupuestarios por día/mes y alertas por umbral.
3. Perfiles por proyecto/sesión, con precedencia clara y controles de seguridad.
4. Cambio **manual o explícitamente autorizado** a un modelo alternativo durante peak.
5. Soporte para más proveedores con tarifas por horario.
6. Interfaz gráfica opcional, sin reemplazar el servicio ligero.
7. Linux/macOS con empaquetado nativo.
8. Métricas agregadas y exportación local sin contenidos de prompts.

---

# 18. Fuentes técnicas y comprobación

Fuentes consultadas para orientar la implementación al 14/10/2026. Revalidar antes de integrar versiones reales:

1. **DeepSeek, Models & Pricing:** https://api-docs.deepseek.com/quick_start/pricing/ — horarios, tarifas y excepción oficial de festivos de China para la API directa (según la página consultada; revisar cambios).
2. **DeepSeek, V4.1 Flash:** https://api-docs.deepseek.com/news/news260910/ — aliases, precios y transición de modelos.
3. **OpenCode Go:** https://opencode.ai/v2/docs/console/go/ — planes, modelos y horarios/precios que factura Go.
4. **OpenCode SDK:** https://opencode.ai/docs/sdk/ — incluye `session.abort`.
5. **OpenCode Plugins:** https://opencode.ai/docs/plugins/ — hooks y extensiones.
6. **OpenCode V2 CLI Plugins:** https://dev.opencode.ai/v2/docs/build/plugins/cli — slots de sidebar, estados y toasts.
7. **Pi, Extensions:** https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/extensions.md — API de extensiones, contexto y UI; validar `ctx.abort` contra versión instalada.
8. **Hermes, Slash Commands:** https://github.com/NousResearch/hermes-agent/blob/main/website/docs/reference/slash-commands.md — comportamiento `/stop`.
9. **Hermes, CLI:** https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/cli.md — Stop e interrupciones de herramientas.
10. **Hermes, Background Delegation:** https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/delegation.md — alcance de interrupción de subagentes.

---

# 19. Resumen ejecutivo de la decisión arquitectónica

**Construir PeakGuard como un controlador autónomo local en Rust, no como un MCP puro.** El **gateway** impide solicitudes facturables protegidas; los **adaptadores** identifican sesión/modelo, detienen el turno con el Stop nativo cuando sea seguro y muestran avisos en la TUI. **MCP es opcional** y no determina si una solicitud pasa o se bloquea.

**Modo recomendado:** `restricted_stop`. Al comenzar peak se bloquean inmediatamente nuevas solicitudes cubiertas, se invoca `session.abort()` / `ctx.abort()` / equivalente seguro, se mantiene abierta la aplicación y la sesión, y al terminar peak se levanta la restricción **sin reanudar automáticamente**.

**Primer producto completo:** Windows + Rust + DeepSeek directo + OpenCode + sidebar + avisos + modo estricto/restringido/solo alertas. OpenCode Go, Pi y Hermes se agregan **solo tras verificar capacidades reales**.

**Indicador de éxito principal:** el usuario recibe avisos oportunos y puede confiar en que el modo de protección elegido se aplica a **todas las rutas explícitamente declaradas como protegidas**, sin cerrar su agente, borrar conversaciones ni afectar modelos que no estén sujetos a la política.
