# PeakGuard — Backlog Futuro (Versión 1.1+)

Este documento recopila las propuestas de evolución arquitectónica y funcional de PeakGuard planificadas para versiones posteriores al MVP (v1.0), conforme a la sección §17 de la Especificación del Plan Maestro.

---

## 1. Estimador de Costo y Ahorro por Tokens Reales (§17.1)
- **Objetivo:** Registrar las cabeceras de respuesta o metadatos de uso (`usage.total_tokens`, `usage.prompt_tokens`, `usage.completion_tokens`) de las peticiones que cruzaron el Gateway antes y después de los límites de Peak.
- **Diferenciación estricta:** Distinguir claramente en UI y reportes entre "Facturación oficial del proveedor" y "Estimación local aproximada de ahorro".
- **Privacidad:** No registrar contenido de prompts ni respuestas; computar únicamente agregados numéricos de tokens y ahorro acumulado.

---

## 2. Límites Presupuestarios por Día y Mes (§17.2)
- **Objetivo:** Permitir definir en `peakguard.toml` umbrales de presupuesto máximo (ej. `$5.00/día` o `$50.00/mes`).
- **Comportamiento:**
  - Alerta preventiva al alcanzar el 80% del umbral vía Desktop y Telegram.
  - Bloqueo o solicitud de confirmación interactiva al alcanzar el 100%.

---

## 3. Perfiles de Configuración por Proyecto o Sesión (§17.3)
- **Objetivo:** Permitir archivos `.peakguard.toml` en la raíz de repositorios de trabajo que sobreescriban parámetros generales (como el `mode` de operación específico para un proyecto de investigación vs producción).
- **Seguridad:** Precedencia clara: la política global del usuario siempre puede fijar techos de seguridad inviolables por proyectos locales.

---

## 4. Sustitución Asistida / Alternativa de Modelo durante Peak (§17.4)
- **Objetivo:** Cuando una petición a `deepseek-chat` sea detenida o bloqueada en modo interactivo, ofrecer al usuario la opción de redirigir temporalmente la petición a un modelo alternativo no sujeto a horario peak (ej. un modelo local Ollama o un proveedor alternativo autorizado).
- **Regla estricta:** Jamás autoconmutar sin consentimiento explícito del usuario humano para evitar costos o respuestas inesperadas.

---

## 5. Soporte para Más Proveedores con Tarifas Dinámicas por Horario (§17.5)
- **Objetivo:** Integrar esquemas de tarifas horarias de otros proveedores de inferencia en la nube que adopten esquemas de precios diferenciados (peak/off-peak).
- **Arquitectura:** Extender el motor de `ScheduleEngine` para asociar perfiles de precios y calendarios de festivos independientes por proveedor.

---

## 6. Soporte Multiplataforma Nativo: Linux y macOS (§17.6)
- **Linux:**
  - Integración con `systemd --user` para inicio automático.
  - Notificaciones de escritorio mediante `libnotify` (`notify-send`) o D-Bus.
- **macOS:**
  - Agente de usuario `launchd` en `~/Library/LaunchAgents/`.
  - Notificaciones nativas mediante AppleScript (`display notification`) o NotificationCenter API.

---

## 7. Métricas Locales y Exportador Prometheus (§17.7)
- **Objetivo:** Exponer opcionalmente un endpoint de métricas `/metrics` en el plano de control (restringido a loopback con autenticación).
- **Métricas expuestas:**
  - `peakguard_requests_total{provider, model, status}`
  - `peakguard_blocked_requests_total{provider, mode}`
  - `peakguard_active_streams`
  - `peakguard_latency_seconds_bucket`
- **Garantía P6:** Ninguna métrica incluirá IDs de sesión sensibles, tokens o textos de entrada.
