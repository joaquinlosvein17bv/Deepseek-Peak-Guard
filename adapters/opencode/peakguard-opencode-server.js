// @ts-nocheck
/**
 * PeakGuard Server Adapter para OpenCode (v1.18.35+)
 * Plugin de servidor/headless para opencode.jsonc ("plugin": ["..."])
 * 
 * Funcionalidad:
 * 1. Autenticación con Bearer Token desde entorno o ~/.peakguard/control.token
 * 2. Registro y seguimiento de sesiones activas en el demonio PeakGuard
 * 3. Suscripción a eventos SSE (/api/v1/events)
 * 4. Cancelación limpia de turnos mediante client.session.abort() sin matar el proceso ni borrar historial
 * 5. Envío de acuses Stop ACK con estado estricto (sin falsos positivos)
 */

import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";
const SOURCE = "peakguard:opencode:server";

let cachedToken = null;

/**
 * Obtiene el token de autenticación para la Control API
 */
export function getAuthToken() {
  if (cachedToken) return cachedToken;
  if (process.env.PEAKGUARD_CONTROL_TOKEN) {
    cachedToken = process.env.PEAKGUARD_CONTROL_TOKEN.trim();
    return cachedToken;
  }
  const tokenFile = path.join(os.homedir(), ".peakguard", "control.token");
  try {
    if (fs.existsSync(tokenFile)) {
      cachedToken = fs.readFileSync(tokenFile, "utf-8").trim();
      return cachedToken;
    }
  } catch {
    // Si no se puede leer el archivo, continuará sin token
  }
  return null;
}

/**
 * Helper para solicitudes HTTP autenticadas hacia la Control API
 */
export async function sendControlRequest(endpoint, method = "GET", body = null) {
  const token = getAuthToken();
  const headers = {
    "Content-Type": "application/json",
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const payload = body ? JSON.stringify(body) : null;
  if (payload) {
    headers["Content-Length"] = Buffer.byteLength(payload);
  }

  const url = new URL(`${PEAKGUARD_CONTROL_URL}${endpoint}`);

  return new Promise((resolve) => {
    const req = http.request(
      url,
      {
        method,
        headers,
      },
      (res) => {
        let data = "";
        res.on("data", (chunk) => (data += chunk));
        res.on("end", () => {
          try {
            resolve({
              statusCode: res.statusCode,
              body: data ? JSON.parse(data) : null,
            });
          } catch {
            resolve({ statusCode: res.statusCode, body: data });
          }
        });
      }
    );
    req.on("error", (err) => resolve({ statusCode: 0, error: err }));
    if (payload) req.write(payload);
    req.end();
  });
}

/**
 * Registra una sesión activa en el daemon
 */
export async function registerActiveSession(sessionID, model = "deepseek-chat", provider = "deepseek_direct", turnID = null) {
  return sendControlRequest("/api/v1/sessions/register", "POST", {
    session_id: sessionID,
    client_type: "opencode",
    turn_id: turnID,
    model,
    provider_id: provider,
    is_active: true,
  });
}

/**
 * Envía latido (heartbeat) de la sesión activa
 */
export async function sendSessionHeartbeat(sessionID, turnID = null, isActive = true) {
  return sendControlRequest("/api/v1/sessions/heartbeat", "POST", {
    session_id: sessionID,
    turn_id: turnID,
    is_active: isActive,
  });
}

/**
 * Desregistra una sesión al cerrarse
 */
export async function unregisterSession(sessionID) {
  return sendControlRequest("/api/v1/sessions/unregister", "POST", {
    session_id: sessionID,
  });
}

/**
 * Envía acuse de recibo de Stop hacia el daemon PeakGuard
 * Rechazo estricto: success DEBE ser un booleano explícito.
 */
export async function sendStopAck(sessionID, transitionUtc, success, error = null, turnID = null) {
  return sendControlRequest("/api/v1/adapters/stop-ack", "POST", {
    session_id: sessionID,
    turn_id: turnID,
    transition_utc: transitionUtc || new Date().toISOString(),
    success: Boolean(success),
    error: error || null,
  });
}

/**
 * Conecta al stream de eventos SSE de PeakGuard para recibir órdenes de Stop nativo
 */
export function startEventSubscriber(client) {
  const token = getAuthToken();
  const queryParam = token ? `?token=${encodeURIComponent(token)}` : "";
  const headers = {};
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const url = `${PEAKGUARD_CONTROL_URL}/api/v1/events${queryParam}`;

  const req = http.get(url, { headers }, (res) => {
    if (res.statusCode !== 200) {
      console.warn(`[PeakGuard:OpenCode] SSE rechazado por Control API (Status ${res.statusCode}). Reintentando...`);
      setTimeout(() => startEventSubscriber(client), 5000);
      return;
    }

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
              const turnID = event.turn_id || null;
              console.warn(
                `[PeakGuard:OpenCode] Recibida orden de Stop nativo para sesión '${sessionID}'. Abortando turno activo...`
              );

              let abortSuccess = false;
              let abortError = null;

              // Cancelación segura mediante API nativa de OpenCode (session.abort)
              // NUNCA matar el proceso de la aplicación ni borrar el historial
              if (client?.session?.abort) {
                try {
                  await client.session.abort({ path: { id: sessionID } });
                  abortSuccess = true;
                  console.info(
                    `[PeakGuard:OpenCode] Turno abortado exitosamente mediante client.session.abort({ id: '${sessionID}' }).`
                  );
                } catch (err) {
                  abortSuccess = false;
                  abortError = err?.message || String(err);
                  console.error(`[PeakGuard:OpenCode] Error invocando client.session.abort:`, err);
                }
              } else {
                abortSuccess = false;
                abortError = "client.session.abort no está disponible en este entorno";
                console.error(`[PeakGuard:OpenCode] ${abortError}`);
              }

              // Enviar acuse con el estado REAL (sin falsos éxitos)
              await sendStopAck(sessionID, event.transition_utc, abortSuccess, abortError, turnID);
            }
          } catch {
            // Ignorar líneas no JSON o comentarios keepalive
          }
        }
      }
    });

    res.on("end", () => {
      setTimeout(() => startEventSubscriber(client), 3000);
    });
  });

  req.on("error", () => {
    setTimeout(() => startEventSubscriber(client), 5000);
  });

  return req;
}

/**
 * Hook de servidor (V1/V2 server plugin para OpenCode 1.18.35+)
 */
export const PeakGuardServerPlugin = async (input) => {
  const client = input?.client;
  startEventSubscriber(client);

  return {
    event: async ({ event }) => {
      const type = event?.type;
      const properties = event?.properties ?? {};
      const sessionID = properties?.sessionID || properties?.info?.id;
      const model = properties?.info?.model || properties?.model || "deepseek-chat";

      if (type === "session.created" && sessionID) {
        await registerActiveSession(sessionID, model);
      }

      if (type === "message.created" && sessionID) {
        await sendSessionHeartbeat(sessionID, properties?.messageId, true);
      }

      if (type === "session.deleted" && sessionID) {
        await unregisterSession(sessionID);
      }
    },
  };
};

export default {
  id: "peakguard.opencode.server",
  server: PeakGuardServerPlugin,
};
