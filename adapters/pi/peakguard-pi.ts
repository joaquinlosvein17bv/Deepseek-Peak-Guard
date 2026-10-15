/**
 * PeakGuard Extension para Pi Coding Agent (0.87.1+)
 * Ubicación recomendada: ~/.pi/agent/extensions/peakguard.ts
 *
 * Implementa la integración real conforme a la API de extensiones de Pi:
 * - Registra callbacks con pi.on("session_start"), pi.on("agent_start"), pi.on("turn_start").
 * - Retiene el contexto activo del turno para invocar ctx.abort() o pi.abort() sin matar el proceso.
 * - Reporta acuses de recibo (Stop ACK) distinguiendo éxitos de fallos reales (sin falsos positivos).
 */

import http from "node:http";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";
const SOURCE = "peakguard:pi";

export async function getPeakGuardStatus(): Promise<any> {
  return new Promise((resolve) => {
    http
      .get(`${PEAKGUARD_CONTROL_URL}/api/v1/status`, (res) => {
        let data = "";
        res.on("data", (chunk) => (data += chunk));
        res.on("end", () => {
          try {
            resolve(JSON.parse(data));
          } catch {
            resolve(null);
          }
        });
      })
      .on("error", () => resolve(null));
  });
}

export async function sendStopAck(
  sessionID: string,
  transitionUtc: string,
  success: boolean,
  error?: string
): Promise<boolean> {
  const payload = JSON.stringify({
    session_id: sessionID,
    transition_utc: transitionUtc || new Date().toISOString(),
    success,
    error: error || null,
  });

  return new Promise((resolve) => {
    const req = http.request(
      `${PEAKGUARD_CONTROL_URL}/api/v1/adapters/stop-ack`,
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(payload),
        },
      },
      (res) => resolve(res.statusCode === 200)
    );
    req.on("error", () => resolve(false));
    req.write(payload);
    req.end();
  });
}

export default function peakguardExtension(pi: any) {
  let activeCtx: any = null;
  let isPeakActive = false;
  let sseReq: http.ClientRequest | null = null;

  // 1. Mantener referencia al contexto de sesión y turno activo
  if (typeof pi?.on === "function") {
    pi.on("session_start", (_event: any, ctx: any) => {
      activeCtx = ctx;
      updateStatusDisplay();
    });

    pi.on("agent_start", (_event: any, ctx: any) => {
      activeCtx = ctx;
      updateStatusDisplay();
    });

    pi.on("turn_start", (_event: any, ctx: any) => {
      activeCtx = ctx;
    });

    pi.on("agent_settled", (_event: any, ctx: any) => {
      if (activeCtx === ctx) {
        // El turno terminó
      }
    });
  }

  // 2. Actualizar indicador en UI
  async function updateStatusDisplay() {
    const status = await getPeakGuardStatus();
    if (!status) return;

    isPeakActive = status.is_peak;
    const label = isPeakActive
      ? "⛔ PEAK (Protegido)"
      : `🛡️ OFF-PEAK (${status.seconds_to_peak ? Math.floor(status.seconds_to_peak / 60) + "m" : "--"})`;

    if (activeCtx?.ui?.setStatus) {
      activeCtx.ui.setStatus("peakguard", label);
    } else if (pi?.ui?.setStatus) {
      pi.ui.setStatus("peakguard", label);
    }
  }

  updateStatusDisplay();
  const pollInterval = setInterval(updateStatusDisplay, 15000);

  // 3. Conectar a eventos SSE del Control API de PeakGuard
  function connectEvents() {
    sseReq = http.get(`${PEAKGUARD_CONTROL_URL}/api/v1/events`, (res) => {
      let buffer = "";
      res.on("data", async (chunk) => {
        buffer += chunk.toString();
        const lines = buffer.split("\n\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          if (line.startsWith("data: ")) {
            try {
              const event = JSON.parse(line.slice(6));
              if (event.type === "stop_requested") {
                const sessionID = event.session_id || "pi-current-session";
                console.warn(
                  `[PeakGuard:Pi] Orden de Stop recibida para '${sessionID}'. Abortando turno activo...`
                );

                let success = false;
                let errorMsg: string | undefined;

                // Invocación nativa sin matar proceso (Principio P1)
                try {
                  if (activeCtx && typeof activeCtx.abort === "function") {
                    activeCtx.abort();
                    success = true;
                    console.info("[PeakGuard:Pi] activeCtx.abort() ejecutado con éxito.");
                  } else if (typeof pi?.abort === "function") {
                    pi.abort();
                    success = true;
                    console.info("[PeakGuard:Pi] pi.abort() ejecutado con éxito.");
                  } else {
                    errorMsg = "Método abort() no disponible en activeCtx ni pi";
                    console.error(`[PeakGuard:Pi] ${errorMsg}`);
                  }
                } catch (err: any) {
                  success = false;
                  errorMsg = err?.message || String(err);
                  console.error("[PeakGuard:Pi] Excepción al invocar abort():", err);
                }

                if (success) {
                  if (activeCtx?.ui?.notify) {
                    activeCtx.ui.notify(
                      "Turno cancelado por PeakGuard",
                      "Horario peak iniciado. Sesión e historial preservados."
                    );
                  }
                }

                // Enviar acuse con status REAL (sin falsos positivos)
                await sendStopAck(sessionID, event.transition_utc, success, errorMsg);
              } else if (event.type === "peak_started" || event.type === "peak_ended") {
                updateStatusDisplay();
              }
            } catch {
              // Ignore non-json lines
            }
          }
        }
      });

      res.on("end", () => {
        setTimeout(connectEvents, 3000);
      });
    });

    sseReq.on("error", () => {
      setTimeout(connectEvents, 5000);
    });
  }

  connectEvents();

  return () => {
    clearInterval(pollInterval);
    if (sseReq) sseReq.destroy();
  };
}
