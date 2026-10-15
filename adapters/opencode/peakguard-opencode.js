// PeakGuard Adapter para OpenCode (v1.18.35+)
// Ubicación recomendada: ~/.config/opencode/plugins/peakguard-opencode.js

import http from "node:http";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";
const SOURCE = "peakguard:opencode";

let activeSessions = new Map(); // sessionID -> { model, lastActive }

/**
 * Consulta el estado actual de PeakGuard
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
 */
export async function sendStopAck(sessionID, transitionUtc) {
  const payload = JSON.stringify({
    session_id: sessionID,
    transition_utc: transitionUtc || new Date().toISOString(),
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
  const req = http.get(`${PEAKGUARD_CONTROL_URL}/api/v1/events`, (res) => {
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
                `[PeakGuard] Recibida orden de Stop para sesión '${sessionID}'. Cancelando turno activo...`
              );

              // Cancelación mediante API nativa de OpenCode (sin cerrar app ni proceso)
              if (client?.session?.abort) {
                try {
                  await client.session.abort({ path: { id: sessionID } });
                  console.info(`[PeakGuard] Turno abortado exitosamente con session.abort({ id: '${sessionID}' }).`);
                } catch (err) {
                  console.error(`[PeakGuard] Error invocando session.abort:`, err);
                }
              }

              // Enviar acuse de recibo al daemon
              await sendStopAck(sessionID, event.transition_utc);
            }
          } catch {
            // Evento no JSON o heartbeat
          }
        }
      }
    });
  });

  req.on("error", () => {
    // Reintento silencioso en 5 segundos si el daemon no está activo
    setTimeout(() => startEventSubscriber(client), 5000);
  });
}

export const PeakGuardServerPlugin = async ({ client }) => {
  startEventSubscriber(client);

  return {
    event: async ({ event }) => {
      const type = event?.type;
      const properties = event?.properties ?? {};
      const sessionID = properties.sessionID;

      if (!sessionID) return;

      switch (type) {
        case "session.created":
        case "session.updated":
          activeSessions.set(sessionID, {
            model: properties.model || "unknown",
            lastActive: Date.now(),
          });
          break;
        case "session.deleted":
          activeSessions.delete(sessionID);
          break;
        default:
          break;
      }
    },
  };
};

export default {
  id: "peakguard.opencode",
  server: PeakGuardServerPlugin,
  setup() {},
};
