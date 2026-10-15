# Política y Modelo de Seguridad de PeakGuard

**Versión:** 1.0  
**Fecha:** 14 de octubre de 2026

---

## 1. Modelo de Amenazas y Principios

PeakGuard se ejecuta íntegramente como un proceso local en la máquina del usuario. Sus objetivos de seguridad son:
1. **Evitar exposición de secretos:** No almacenar ni registrar en texto plano claves de API (DeepSeek, OpenCode, Telegram) ni tokens de control.
2. **Prevenir ataques de canal cruzado / navegador (DNS Rebinding / CSRF):** Evitar que páginas web maliciosas en el navegador del usuario envíen peticiones a los puertos locales de PeakGuard (`127.0.0.1:8765` o `127.0.0.1:8766`).
3. **Mínimos privilegios:** La ejecución ordinaria, instalación y operación **nunca** requieren permisos de Administrador de Windows ni instalación de certificados raíz / MITM.
4. **Integridad de ejecución del agente:** PeakGuard nunca invoca `taskkill`, `process.kill` ni cierra procesos de OpenCode, Pi o Hermes; solo envía comandos de cancelación de turno (`session.abort()`).

---

## 2. Controles de Seguridad Implementados

### 2.1. Aislamiento en Loopback
- Tanto el Gateway (`127.0.0.1:8765`) como la Control API (`127.0.0.1:8766`) se vinculan **estrictamente a la interfaz IPv4 de loopback (`127.0.0.1`)**.
- Está prohibido vincularse a `0.0.0.0` o interfaces de red externas.

### 2.2. Autenticación del Plano de Control
- Toda petición a la Control API (`127.0.0.1:8766`) debe incluir el encabezado:
  `Authorization: Bearer <PEAKGUARD_CONTROL_TOKEN>`
- El token de control es una cadena criptográficamente aleatoria generada durante el primer arranque y almacenada en memoria o en un archivo con permisos restringidos al usuario actual de Windows.

### 2.3. Protección contra Navegadores (CORS & Host Header)
- Se valida el header `Host`. Si contiene nombres de dominio externos o sospechosos, la petición es rechazada con HTTP 400.
- No se emiten encabezados `Access-Control-Allow-Origin: *`. Solicitudes con `Origin` correspondiente a navegadores web son rechazadas.

### 2.4. Redacción Estricta de Logs
- Los encabezados `Authorization`, `Proxy-Authorization`, `X-Api-Key` y variables que contengan `TOKEN` o `KEY` se ofuscan automáticamente en los registros (`Bearer ********`).
- **Prohibido registrar prompts:** El cuerpo (`body`) de las peticiones de inferencia y las respuestas generadas por los modelos de IA nunca se almacenan en los archivos de log. Solo se registran metadatos no sensibles (`timestamp`, `request_id`, `provider_id`, `model_id`, `decision`, `duration_ms`).

### 2.5. Validación de Destinos Upstream (Allowlist Estricta)
- El gateway solo reenvía tráfico a upstreams explícitamente autorizados en la configuración (`https://api.deepseek.com`, `https://opencode.ai`).
- No actúa como proxy abierto (Open Proxy); cualquier petición dirigida a otros destinos es rechazada de inmediato para prevenir SSRF.
