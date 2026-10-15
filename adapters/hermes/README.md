# Adaptador e Integración de PeakGuard para Hermes Agent

**Versión:** 1.0  
**Estado:** Integración por Gateway Verificada; Stop Nativo Degradado por Seguridad

---

## 1. Análisis de Alcance del Comando `/stop` en Hermes

Conforme a la auditoría de T00:
- El comando `/stop` en Hermes interrumpe no solamente el turno de inferencia activo, sino también tareas y subagentes en segundo plano (`delegation`).
- **Decisión de Seguridad (Principio P1):** PeakGuard **no automatiza la inyección de `/stop` ciegamente en Hermes**, ya que podría detener tareas no relacionadas o subprocesos concurrentes legítimos.
- **Mecanismo Primario:** **Bloqueo directo en Gateway HTTP local.** Al cortar la conexión a nivel de gateway antes de que llegue a internet, el SDK de OpenAI que utiliza Hermes recibe un error 403 `peakguard_policy_blocked` y finaliza el turno sin afectar el daemon ni otros subprocesos.

---

## 2. Configuración en Hermes

Modifique el archivo `C:\Users\<Usuario>\AppData\Local\hermes\config.yaml`:

```yaml
model:
  default: deepseek-chat
  provider: deepseek
  base_url: http://127.0.0.1:8765/v1
  api_mode: chat_completions
```

---

## 3. Monitor de Estado para la Terminal de Hermes

Se incluye el script ligero `peakguard-hermes-monitor.ps1` para mostrar el estado en tiempo real en la terminal o canal kanban de Hermes:

```powershell
.\adapters\hermes\peakguard-hermes-monitor.ps1
```
