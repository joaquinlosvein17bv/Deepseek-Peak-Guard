# Especificación Formal de Horarios, Intervalos y Deduplicación

**Módulo:** `schedule` / `clock`  
**Versión de especificación:** 1.0  
**Fecha:** 14 de octubre de 2026

---

## 1. Semántica Temporal Fundamental

### 1.1. Fuente Única de Verdad: UTC
Todas las evaluaciones de política, cálculos de transiciones y comparaciones horarias se realizan estrictamente en **UTC (Tiempo Universal Coordinado)**.  
Las zonas horarias locales (por ejemplo, `America/Lima` UTC−5 o zonas con horario de verano DST como `America/New_York`) son de **uso exclusivo de presentación al usuario** en interfaces TUI, logs formateados o mensajes de Telegram.  
Ningún cambio en el reloj local o en la configuración de zona horaria del sistema operativo puede alterar la clasificación de una solicitud.

### 1.2. Definición de Intervalos: Semirregular Semiabierto `[inicio, fin)`
Todo intervalo peak está definido como un rango cerrado a la izquierda y abierto a la derecha:
$$\text{peak} \iff t \ge t_{\text{inicio}} \land t < t_{\text{fin}}$$

**Casos de frontera exactos:**
- $t = 01:00:00.000\text{ UTC} \implies \mathbf{PEAK}$
- $t = 03:59:59.999\text{ UTC} \implies \mathbf{PEAK}$
- $t = 04:00:00.000\text{ UTC} \implies \mathbf{OFF\_PEAK}$
- $t = 06:00:00.000\text{ UTC} \implies \mathbf{PEAK}$
- $t = 09:59:59.999\text{ UTC} \implies \mathbf{PEAK}$
- $t = 10:00:00.000\text{ UTC} \implies \mathbf{OFF\_PEAK}$

### 1.3. Fines de Semana
El fin de semana comprende desde el sábado a las 00:00:00.000 UTC hasta el domingo a las 23:59:59.999 UTC inclusive.
Durante todo el fin de semana, el estado es **OFF-PEAK** independientemente de la hora del día.

---

## 2. Perfiles por Proveedor

### 2.1. `deepseek_direct`
- **Días aplicables:** Lunes a Viernes (UTC).
- **Intervalos diarios:**
  - Intervalo 1: `01:00` a `04:00` UTC
  - Intervalo 2: `06:00` a `10:00` UTC
- **Festivos:** Excluye días festivos oficiales de la República Popular China (declarados por el Consejo de Estado). Durante esos días festivos oficiales, el estado es **OFF-PEAK**.
- **Equivalencia local en Perú (`America/Lima`, UTC−5):**
  - Peak 1: 20:00 a 23:00 (del día local anterior: domingo a jueves).
  - Peak 2: 01:00 a 05:00 (del día local: lunes a viernes).

### 2.2. `opencode_go`
- **Días aplicables:** Lunes a Viernes (UTC).
- **Intervalos diarios:**
  - Intervalo 1: `01:00` a `04:00` UTC
  - Intervalo 2: `06:00` a `10:00` UTC
- **Festivos:** `exclude_chinese_public_holidays = false`. La documentación pública de OpenCode Go no especifica dicha excepción; no se asume paridad sin verificación explícita.

---

## 3. Umbrales de Notificación y Deduplicación

### 3.1. Umbrales Preventivos
Para cada transición hacia un estado PEAK con hora de inicio $T_0$:
- **T−30 min** ($T_0 - 1800\text{ s}$): Aviso preventivo informativo.
- **T−10 min** ($T_0 - 600\text{ s}$): Advertencia moderada.
- **T−5 min** ($T_0 - 300\text{ s}$): Alerta prioritaria indicando modo activo.
- **T−10 s a T−1 s**: Cuenta regresiva activa segundo a segundo en TUI.
- **T=0**: Disparo de la política seleccionada (`strict_block` o `restricted_stop`).
- **Fin de Peak**: Aviso de fin de franja cara y retorno a OFF-PEAK.

### 3.2. Clave Canónica de Deduplicación
Para evitar notificaciones repetidas en caso de reconexiones, reinicios del servicio o suspensiones del equipo:
$$\text{DeduplicationKey} = (\text{schedule\_id}, \text{window\_start\_utc}, \text{threshold\_id}, \text{channel})$$
Donde:
- `schedule_id`: Identificador de la regla horaria (`deepseek_direct_verified`).
- `window_start_utc`: Marca de tiempo ISO-8601 del inicio exacto de la ventana peak.
- `threshold_id`: `"warning_30"`, `"warning_10"`, `"warning_5"`, `"countdown_start"`, `"peak_active"`, `"peak_ended"`.
- `channel`: `"tui"`, `"desktop"`, `"telegram"`.

Si el sistema inicia en $T_0 - 4\text{ min}$, las alertas de 30 y 10 minutos se consideran expiradas y no se emiten retroactivamente.
