# Registro de Decisiones de Arquitectura (ADRs)

---

## ADR-001: Núcleo Autónomo en Rust con Gateway HTTP Local y Adaptadores Específicos

### Fecha
14 de octubre de 2026

### Estado
Aceptado

### Contexto
Se requiere proteger contra cobros excesivos y saturación en horas pico de DeepSeek (01:00–04:00 y 06:00–10:00 UTC) sin cerrar las aplicaciones del usuario (OpenCode, Pi, Hermes) ni perder el historial de conversación.

Existían tres opciones conceptuales:
1. **MCP Puro:** Crear exclusivamente un servidor MCP (Model Context Protocol).
2. **Modificación de Red a Nivel de Sistema Operativo:** Modificar archivo `hosts` o reglas de Windows Firewall (como hacía el script heredado `peakguard.py`).
3. **Núcleo Local en Rust con Gateway HTTP + API de Control + Adaptadores por Cliente + MCP Opcional:** Un binario compilado ultra-ligero que ofrece un gateway local loopback para interceptar peticiones de inferencia, API de eventos/control para plugins y MCP de consulta opcional.

### Decisión
Se adopta la **Opción 3**:
1. **El protocolo MCP no es suficiente como mecanismo de control:** Un servidor MCP expone herramientas que el modelo decide o no invocar. No se interpone entre la llamada HTTP del cliente a DeepSeek. Por tanto, MCP queda como una interfaz complementaria y opcional de solo consulta (`status`, `next_peak`, `models`, `coverage`).
2. **El bloqueo de red a nivel de Firewall/Hosts es descartado para el MVP:** Bloquear hosts o IPs afecta a todos los procesos sin discriminar si una petición va dirigida a un modelo no protegido, y requiere privilegios elevados de Administrador.
3. **Gateway HTTP Loopback en Rust (`127.0.0.1:8765`):** Permite inspeccionar en tiempo real el modelo en el payload (`model`), evaluar la ventana horaria UTC de forma determinista y rechazar con un error estructurado antes de que la petición salga a internet.
4. **Adaptadores específicos para Stop Nativo:**
   - **OpenCode:** Plugin TUI/Server que ejecuta `session.abort()` al recibir la señal de entrada en peak, dejando la aplicación abierta y el historial intacto.
   - **Pi:** Extensión que usa `ctx.abort()` sin invocar `ctx.shutdown()`.
   - **Hermes:** Integración primaria vía Gateway HTTP; `/stop` se condiciona a aislamiento de tareas en background para evitar efectos secundarios.
5. **Políticas de Festivos Independientes:** La exclusión de festivos chinos de la API directa de DeepSeek NO se aplica automáticamente a otros proveedores o resellers como OpenCode Go. Cada proveedor tiene su propio perfil de horario versionado.

### Consecuencias
- **Positivas:**
  - Cero dependencias pesadas en tiempo de ejecución; consumo mínimo de CPU (<1%) y RAM.
  - La aplicación nunca se cierra inesperadamente (`abort` nativo vs `taskkill`).
  - Cobertura verificable y honesta: el usuario sabe exactamente qué rutas están protegidas y cuáles no.
- **Negativas / Desafíos:**
  - Requiere configurar los clientes para apuntar a la URL base del gateway local (`http://127.0.0.1:8765/v1`) para rutas no interceptadas por plugins.
  - Se deben mantener adaptadores ligeros en TypeScript/JavaScript para OpenCode y Pi.
