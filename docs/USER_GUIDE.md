# Guía Completa de Usuario de PeakGuard

**Versión:** 1.0  
**Plataforma:** Windows 10 / Windows 11 x64

---

## 1. ¿Qué es PeakGuard?

PeakGuard es un guardián local ultra-ligero (desarrollado en Rust) que supervisa los horarios de mayor costo o saturación (*Peak*) de los modelos de inteligencia artificial (principalmente DeepSeek).

Durante las horas Peak oficiales (Lunes a Viernes de 01:00 a 04:00 y de 06:00 a 10:00 UTC), las peticiones a la API directa de DeepSeek cuestan el doble que en horario Off-Peak. PeakGuard evita cobros excesivos deteniendo oportunamente el turno de tu asistente de programación (OpenCode, Pi, Hermes) **sin cerrar tu aplicación ni borrar tu historial de conversación**.

---

## 2. Los Tres Modos de Operación

En `peakguard.toml`, puedes configurar el modo mediante la clave `mode`:

1. **`restricted_stop` (Recomendado / Predeterminado):**
   - Al llegar el horario peak, emite una orden de interrupción nativa (`session.abort()` en OpenCode, `ctx.abort()` en Pi) para detener el turno activo en el asistente.
   - La aplicación y la conversación permanecen 100% abiertas e interactivas.
   - Bloquea nuevas peticiones dirigidas a modelos protegidos en el Gateway local hasta que termine el horario peak.
2. **`strict_block` (Bloqueo Estricto):**
   - Bloquea cualquier petición o stream hacia el proveedor protegido en el Gateway local con código HTTP 403 `peakguard_policy_blocked`.
   - Adecuado para aplicaciones que no cuentan con un plugin de abort nativo.
3. **`notify_only` (Solo Alertas):**
   - No bloquea ninguna petición ni cancela turnos.
   - Emite avisos preventivos a los 30, 10 y 5 minutos antes, y muestra la cuenta regresiva de 10 segundos en la barra lateral o Telegram.

---

## 3. Inicio Rápido

### Paso 1: Validar Configuración
```powershell
.\peakguard.exe config validate "config\peakguard.example.toml"
```

### Paso 2: Ejecutar Diagnóstico de Entorno
```powershell
.\peakguard.exe doctor --config "config\peakguard.example.toml"
```
El comando `doctor` verificará la disponibilidad de puertos (`8765` para el Gateway y `8766` para la Control API) y detectará las aplicaciones instaladas en tu sistema (OpenCode, Pi, Hermes).

### Paso 3: Iniciar el Demonio
```powershell
.\peakguard.exe run --config "config\peakguard.example.toml"
```
Para detener el demonio de forma limpia en cualquier momento, presione `Ctrl+C`.

---

## 4. Configuración de Clientes

### OpenCode (TUI & Stop Nativo)
1. Copie el plugin del adaptador a su carpeta de plugins de OpenCode:
   ```powershell
   Copy-Item adapters\opencode\peakguard-opencode.js $env:USERPROFILE\.config\opencode\plugins\
   Copy-Item adapters\opencode\peakguard-tui-sidebar.js $env:USERPROFILE\.config\opencode\plugins\
   ```
2. En `~/.config/opencode/opencode.jsonc`, añada los plugins a la lista `"plugin"`:
   ```jsonc
   "plugin": [
     "./plugins/peakguard-opencode.js",
     "./plugins/peakguard-tui-sidebar.js"
   ]
   ```
3. En la barra lateral derecha de OpenCode verá el indicador en vivo:
   `PeakGuard [● OFF-PEAK] Cuenta atrás: 00:29:45`

### Pi Coding Agent
1. Copie la extensión a Pi:
   ```powershell
   Copy-Item adapters\pi\peakguard-pi.ts $env:USERPROFILE\.pi\agent\extensions\
   ```
2. En `~/.pi/agent/models.json`, apunte la `baseUrl` del proveedor a `http://127.0.0.1:8765/v1`.

### Hermes Agent
1. En `C:\Users\<Usuario>\AppData\Local\hermes\config.yaml`, configure:
   ```yaml
   model:
     base_url: http://127.0.0.1:8765/v1
   ```

---

## 5. Configuración de Notificaciones por Telegram (Opcional)

1. En `peakguard.toml`, active `telegram = true` en la sección `[alerts]`.
2. Establezca sus variables de entorno o defínalas en su perfil:
   ```powershell
   $env:PEAKGUARD_BOT_TOKEN = "123456789:ABCDef..."
   $env:PEAKGUARD_CHAT_ID = "987654321"
   ```
3. PeakGuard enviará automáticamente las alertas preventivas (30 min, 10 min, 5 min) y el aviso al finalizar el horario peak.

---

## 6. Desinstalación Limpia

PeakGuard es un sistema totalmente autónomo que no instala controladores en el kernel ni modifica el registro de Windows.
Para desinstalarlo:
1. Cierre el proceso `peakguard.exe`.
2. Elimine los archivos de plugins de las carpetas de extensiones de OpenCode o Pi si los copió.
3. Restaure las URLs base en los archivos de configuración de sus clientes a los valores por defecto del proveedor.
