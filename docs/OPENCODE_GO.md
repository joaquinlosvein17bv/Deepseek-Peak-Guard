# Informe de Viabilidad e Integración: OpenCode Go

**Fecha de evaluación:** 14 de octubre de 2026  
**Versión de OpenCode:** 1.18.35  
**Estado de Cobertura Oficial:** `PARTIAL` (Stop Nativo Verificado; Gateway condicionado a Custom Provider)

---

## 1. Análisis Técnico del Flujo de OpenCode Go

OpenCode Go opera como un servicio gestionado por el equipo de OpenCode:
- **Endpoint Upstream Oficial:** `https://opencode.ai/zen/go/v1`
- **Gestión de Credenciales:** Token de sesión almacenado en `~/.local/share/opencode/auth.json` bajo la clave `OpenCode Go api`.
- **Mecanismo de Inferencia Interno:** El binario compilado de OpenCode utiliza internamente un cliente HTTP que por defecto apunta al endpoint de Cloudflare de `opencode.ai`.

---

## 2. Hallazgos de Viabilidad de Enrutamiento (Routing)

### Ruta A: Enrutamiento mediante Custom Provider (@ai-sdk/openai-compatible)
- **Factibilidad:** ✅ **COMPROBADA Y VERIFICADA**.
- En `opencode.json`, el usuario puede declarar un proveedor personalizado:
  ```json
  "provider": {
    "peakguard-go": {
      "name": "OpenCode Go (PeakGuard Protected)",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "http://127.0.0.1:8765/v1",
        "headers": {
          "Authorization": "Bearer $OPENCODE_GO_TOKEN"
        }
      }
    }
  }
  ```
- **Garantía:** El tráfico pasa 100% por el Gateway de PeakGuard. Si llega la hora peak, el gateway bloquea la llamada con HTTP 403 y no se genera consumo.
- **Estado de cobertura:** `PROTEGIDO`.

### Ruta B: Uso del comando nativo con provider por defecto `opencode/`
- Si el usuario ejecuta directamente con el prefijo `opencode/deepseek-v4-flash-free` sin usar el custom provider, OpenCode hace la llamada HTTP directamente sin pasar por el loopback local.
- **Mecanismo de mitigación:** El plugin `peakguard-opencode.js` detecta el evento de sesión activa y emite `session.abort()`.
- **Limitación honesta:** Si un subagente interno dispara una petición en el instante exacto antes del abort, esa llamada individual saldrá a internet.
- **Estado de cobertura:** `PARTIAL` (Protegido por Stop Nativo, no por Gateway).

---

## 3. Políticas Horarias de OpenCode Go vs DeepSeek Directo

| Característica | DeepSeek API Directa | OpenCode Go | Regla en PeakGuard |
|---|---|---|---|
| Horarios Peak (UTC) | 01:00–04:00 y 06:00–10:00 | 01:00–04:00 y 06:00–10:00 | Idénticos intervalos UTC |
| Fines de Semana | OFF-PEAK completo | OFF-PEAK completo | Idéntico |
| Festivos Oficiales Chinos | **EXCLUIDOS** (Pasan a off-peak) | **NO EXCLUIDOS** (Texto oficial no lo contempla) | **Perfil Independiente:** `exclude_chinese_public_holidays = false` |
| Tarifa Fuera de Peak | 50% de descuento | 50% de descuento / cuota reducida | Metadato informativo |

---

## 4. Conclusión y Recomendación
PeakGuard clasifica a OpenCode Go como `PARTIAL` por defecto en la UI para garantizar la transparencia exigida en el Principio P3. Para obtener protección total `PROTEGIDO`, se documenta en el README la configuración del custom provider en `opencode.json`.
