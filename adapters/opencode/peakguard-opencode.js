// PeakGuard Adapter para OpenCode (v1.18.35+)
// Compatible con interfaces V1 y V2 (server, setup, tui)
// Ubicación recomendada: ~/.config/opencode/plugins/peakguard-opencode.js

import http from "node:http";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";
const SOURCE = "peakguard:opencode";

let activeSessions = new Map(); // sessionID -> { model, lastActive }

/**
 * Consulta el estado actual de PeakGuard desde el Control API local
 */
export async function getPeakGuardStatus() {
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

/**
 * Envía acuse de recibo de Stop hacia el daemon PeakGuard
 * Diferencia explícitamente éxitos de fallos (sin falsos positivos)
 */
export async function sendStopAck(sessionID, transitionUtc, success, error) {
  const payload = JSON.stringify({
    session_id: sessionID,
    transition_utc: transitionUtc || new Date().toISOString(),
    success: Boolean(success),
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

/**
 * Conecta al stream de eventos SSE de PeakGuard para recibir órdenes de Stop nativo
 */
function startEventSubscriber(client) {
  let sseReq = http.get(`${PEAKGUARD_CONTROL_URL}/api/v1/events`, (res) => {
    let buffer = "";
    res.on("data", async (chunk) => {
      buffer += chunk.toString();
      const lines = buffer.split("\n\n");
      buffer = lines.pop() || "";

      for (const line of lines) {
        if (line.startsWith("data: ")) {
          try {
            const event = JSON.parse(line.slice(6));
            if (event.type === "stop_requested" && event.session_id) {
              const sessionID = event.session_id;
              console.warn(
                `[PeakGuard:OpenCode] Recibida orden de Stop para sesión '${sessionID}'. Cancelando turno activo...`
              );

              let abortSuccess = false;
              let abortError = null;

              // Cancelación mediante API nativa de OpenCode (sin cerrar app ni proceso)
              if (client?.session?.abort) {
                try {
                  await client.session.abort({ path: { id: sessionID } });
                  abortSuccess = true;
                  console.info(
                    `[PeakGuard:OpenCode] Turno abortado exitosamente con session.abort({ id: '${sessionID}' }).`
                  );
                } catch (err) {
                  abortSuccess = false;
                  abortError = err?.message || String(err);
                  console.error(`[PeakGuard:OpenCode] Error invocando session.abort:`, err);
                }
              } else {
                abortSuccess = false;
                abortError = "client.session.abort no está disponible en este contexto";
                console.error(`[PeakGuard:OpenCode] ${abortError}`);
              }

              // Enviar acuse de recibo con el estado REAL (sin falsos éxitos)
              await sendStopAck(sessionID, event.transition_utc, abortSuccess, abortError);
            }
          } catch {
            // Ignorar líneas no JSON o heartbeat
          }
        }
      }
    });

    res.on("end", () => {
      setTimeout(() => startEventSubscriber(client), 3000);
    });
  });

  sseReq.on("error", () => {
    setTimeout(() => startEventSubscriber(client), 5000);
  });
}

/**
 * Hook de servidor (V1/V2 server plugin)
 */
export const PeakGuardServerPlugin = async (input) => {
  const client = input?.client;
  startEventSubscriber(client);

  return {
    event: async ({ event }) => {
      const type = event?.type;
      const properties = event?.properties ?? {};
      const sessionID = properties?.sessionID || properties?.info?.id;

      if (type === "session.created" && sessionID) {
        activeSessions.set(sessionID, {
          model: properties?.info?.model || "unknown",
          lastActive: Date.now(),
        });
      }

      if (type === "session.deleted" && sessionID) {
        activeSessions.delete(sessionID);
      }
    },
  };
};

/**
 * Hook de setup para V1
 */
export function PeakGuardSetupPlugin(api) {
  startEventSubscriber(api?.client);
}

/**
 * Hook de TUI para V2
 */
export async function PeakGuardTuiPlugin(api) {
  // Registra slot de status si la API de slots está disponible
  if (api?.slots?.register) {
    try {
      api.slots.register({
        id: "peakguard-statusline",
        order: 90,
        slots: {
          status_extra() {
            return "[PeakGuard]";
          },
        },
      });
    } catch {
      // Ignorar si el slot no está disponible en esta versión de TUI
    }
  }
}

// Exportación unificada compatible con OpenCode 1.18.35
export default {
  id: "peakguard.opencode",
  server: PeakGuardServerPlugin,
  setup: PeakGuardSetupPlugin,
  tui: PeakGuardTuiPlugin,
};
