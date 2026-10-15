# PeakGuard — Servidor MCP (Model Context Protocol)

Este documento describe la especificación, diseño de seguridad y guía de integración del servidor MCP opcional incluido en el binario de PeakGuard (`peakguard mcp`).

---

## 1. Propósito y Filosofía de Diseño

El servidor MCP de PeakGuard provee herramientas consultivas deterministas a entornos de desarrollo asistidos por IA (OpenCode, Pi Coding Agent, Claude Desktop, Cursor, Windsurf, etc.) mediante el protocolo estándar **JSON-RPC 2.0 sobre `stdio`**.

### Principios Fundamentales:
1. **Solo Lectura por Defecto (§9.4):** Todas las herramientas expuestas son estrictamente consultivas (`read-only`). No consumen tokens de LLM ni invocan modelos externos.
2. **Salvaguarda contra Auto-Desbloqueo (§9.4):** Las herramientas de modificación (`set_mode`, cambio de políticas) están explícitamente deshabilitadas. Un modelo de IA no puede autodesbloquearse o alterar las reglas durante horario peak. El usuario humano retiene el control interactivo exclusivo mediante la CLI (`peakguard config`) o el panel lateral TUI.
3. **Desacoplamiento Absoluto (Criterio AT-22):** Si el servidor MCP no se inicia, se bloquea o es desinstalado, el núcleo de PeakGuard (Gateway HTTP `127.0.0.1:8765`, Control API `127.0.0.1:8766` y motor de alertas) continúa operando con el 100% de su eficacia protectora.

---

## 2. Herramientas Expuestas (`tools/list`)

El servidor MCP publica 4 herramientas estándar:

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `peakguard_status` | *(ninguno)* | Devuelve la hora UTC actual, modo configurado, estado de protección (`ACTIVE_PEAK` vs `OFF_PEAK_SAFE`), tipo de próxima transición y segundos restantes. |
| `peakguard_next_peak` | *(ninguno)* | Provee la fecha/hora UTC exacta del próximo inicio de horario peak, intervalos configurados y tiempo restante legible por humanos. |
| `peakguard_models` | *(ninguno)* | Lista todos los modelos registrados en el catálogo de PeakGuard (`deepseek-chat`, `deepseek-reasoner`, etc.), sus aliases y su nivel de cobertura. |
| `peakguard_coverage` | `model_id` (string, req) | Evalúa si un modelo específico (ej. `deepseek-chat` vs `claude-3-5-sonnet`) está sujeto a políticas de bloqueo o si transita inmune por el Principio P2. |

---

## 3. Ejemplos de Salida de Herramientas

### `peakguard_status`
```json
{
  "utc_now": "2026-10-14T00:00:00+00:00",
  "mode": "restricted_stop",
  "is_peak": false,
  "schedule_id": "deepseek_direct_verified",
  "status": "OFF_PEAK_SAFE",
  "next_transition_type": "peak_start",
  "next_transition_utc": "2026-10-14T01:00:00+00:00",
  "seconds_to_transition": 3600,
  "gateway_endpoint": "http://127.0.0.1:8765",
  "control_api_endpoint": "http://127.0.0.1:8766"
}
```

### `peakguard_coverage`
#### Consulta sobre DeepSeek Chat:
```json
{
  "query_model": "deepseek-chat",
  "resolved_canonical_id": "deepseek-chat",
  "provider_id": "deepseek_direct",
  "is_deepseek_family": true,
  "coverage_through_gateway": "Protected",
  "coverage_direct_bypass": "Uncovered",
  "policy_action_during_peak": "Bloqueo 403 o abort de turno",
  "explanation": "Este modelo pertenece a la familia DeepSeek. Si transita a través del Gateway local (127.0.0.1:8765), estará estrictamente protegido según las ventanas de horario peak..."
}
```

#### Consulta sobre Claude 3.5 Sonnet (Principio P2):
```json
{
  "query_model": "claude-3-5-sonnet",
  "resolved_canonical_id": null,
  "provider_id": null,
  "is_deepseek_family": false,
  "coverage_through_gateway": "Uncovered",
  "coverage_direct_bypass": "Uncovered",
  "policy_action_during_peak": "Permitido (sin bloqueo)",
  "explanation": "Este modelo NO pertenece a DeepSeek (es un modelo de terceros, e.g. Anthropic, OpenAI, Google). Por el Principio P2 de PeakGuard, nunca será bloqueado, detenido ni afectado por ninguna regla de horario peak."
}
```

---

## 4. Configuración en Clientes MCP

### 4.1. OpenCode (`opencode.json` o `~/.config/opencode/mcp.json`)
```json
{
  "mcpServers": {
    "peakguard": {
      "command": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe",
      "args": ["mcp", "--config", "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\config\\peakguard.example.toml"]
    }
  }
}
```

### 4.2. Claude Desktop (`%APPDATA%\Claude\claude_desktop_config.json`)
```json
{
  "mcpServers": {
    "peakguard": {
      "command": "C:\\Users\\meliodaslosven\\Desktop\\Projects\\deepseek\\Deepseek-Peak-Guard\\dist\\peakguard.exe",
      "args": ["mcp"]
    }
  }
}
```

### 4.3. Cursor / Windsurf
Añadir nuevo servidor MCP de tipo `stdio`:
- **Nombre:** `peakguard`
- **Comando:** `peakguard.exe` (o ruta absoluta a `dist/peakguard.exe`)
- **Argumentos:** `mcp`

---

## 5. Desinstalación y Desactivación

Para deshabilitar el servidor MCP:
1. Elimine la entrada `"peakguard"` del archivo de configuración del cliente correspondiente.
2. Guarde el archivo y reinicie el cliente.
3. El Gateway (`127.0.0.1:8765`), el Control API (`127.0.0.1:8766`) y los plugins de TUI continuarán funcionando de forma completamente autónoma sin requerir ningún ajuste adicional.
