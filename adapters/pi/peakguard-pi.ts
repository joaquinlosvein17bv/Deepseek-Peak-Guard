/**
 * PeakGuard Extension para Pi Coding Agent (0.87.1+)
 * Ubicación recomendada: ~/.pi/agent/extensions/peakguard.ts
 */

import http from "node:http";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";

export default function peakguardExtension(ctx: any) {
  let isPeakActive = false;

  // 1. Sincronizar estado y pintar status en la UI de Pi
  async function syncStatus() {
    return new Promise<void>((resolve) => {
      http
        .get(`${PEAKGUARD_CONTROL_URL}/api/v1/status`, (res) => {
          let data = "";
          res.on("data", (chunk) => (data += chunk));
          res.on("end", () => {
            try {
              const status = JSON.parse(data);
              isPeakActive = status.is_peak;

              if (ctx?.ui?.setStatus) {
                const label = isPeakActive
                  ? "⛔ PEAK (Protegido)"
                  : `🛡️ OFF-PEAK (${status.seconds_to_peak ? Math.floor(status.seconds_to_peak / 60) + "m" : "--"})`;
                ctx.ui.setStatus("peakguard", label);
              }
            } catch {
              // ignore
            }
            resolve();
          });
        })
        .on("error", () => resolve());
    });
  }

  syncStatus();
  setInterval(syncStatus, 10000);

  // 2. Conectar a eventos SSE para recibir Stop nativo (ctx.abort)
  const req = http.get(`${PEAKGUARD_CONTROL_URL}/api/v1/events`, (res) => {
    let buffer = "";
    res.on("data", (chunk) => {
      buffer += chunk.toString();
      const lines = buffer.split("\n\n");
      buffer = lines.pop() || "";

      for (const line of lines) {
        if (line.startsWith("data: ")) {
          try {
            const event = JSON.parse(line.slice(6));
            if (event.type === "stop_requested") {
              console.warn("[PeakGuard] Señal de Stop recibida para Pi. Cancelando turno activo...");

              // Ejecutar abort nativo sin cerrar la aplicación (Principio P1: NO ctx.shutdown())
              if (typeof ctx?.abort === "function") {
                ctx.abort();
                if (ctx?.ui?.notify) {
                  ctx.ui.notify(
                    "Turno cancelado por política PeakGuard",
                    "Horario peak activo en modelo DeepSeek. La sesión sigue intacta."
                  );
                }
              }
            }
          } catch {
            // ignore
          }
        }
      }
    });
  });

  req.on("error", () => {
    // Reintento silencioso
  });
}
