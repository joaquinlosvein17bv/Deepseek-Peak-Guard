// @ts-nocheck
/**
 * PeakGuard Server Adapter para OpenCode (v1.18.35+)
 * Plugin de servidor/headless para opencode.jsonc ("plugin": ["..."])
 * 
 * Funcionalidad:
 * 1. Autenticación con Bearer Token desde entorno o ~/.peakguard/control.token
 * 2. Identidad compuesta con CLIENT_INSTANCE_ID único y estable por instancia (T18.1.2)
 * 3. Registro y seguimiento de sesiones activas en el demonio PeakGuard sin modelos ficticios (T18.1.1)
 * 4. Latidos periódicos durante turnos activos para preservar sesiones de razonamiento largo (T18.1.4)
 * 5. Recuperación de órdenes Stop pendientes al conectarse o reconectarse (T18.1.3)
 * 6. Gestión robusta de reconexión con backoff estable y guardia contra reconexiones simultáneas (T18.1.6)
 * 7. Cancelación limpia de turnos mediante client.session.abort() sin matar el proceso ni borrar historial
 * 8. Envío de acuses Stop ACK con estado estricto y command_id (sin falsos positivos)
 */

import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import crypto from "node:crypto";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";
const SOURCE = "peakguard:opencode:server";

// Identidad única y estable por instancia de proceso OpenCode (T18.1.2)
export const CLIENT_INSTANCE_ID = `opencode_inst_${process.pid}_${crypto.randomBytes(6).toString("hex")}`;

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

export const activeSessions = new Map();
const processedStopKeys = new Set();

/**
 * Inicia latido periódico para una sesión en turno activo (T18.1.4)
 */
function startPeriodicHeartbeat(sessionID, turnID) {
  const sess = activeSessions.get(sessionID);
  if (!sess) return;
  if (sess.heartbeatTimer) {
    clearInterval(sess.heartbeatTimer);
  }
  sess.heartbeatTimer = setInterval(async () => {
    if (activeSessions.has(sessionID)) {
      await sendControlRequest("/api/v1/sessions/heartbeat", "POST", {
        session_id: sessionID,
        client_instance_id: CLIENT_INSTANCE_ID,
        turn_id: sess.turnID || turnID,
        is_active: true,
      });
    }
  }, 10000);
}

/**
 * Detiene el latido periódico de una sesión
 */
function stopPeriodicHeartbeat(sessionID) {
  const sess = activeSessions.get(sessionID);
  if (sess && sess.heartbeatTimer) {
    clearInterval(sess.heartbeatTimer);
    sess.heartbeatTimer = null;
  }
}

/**
 * Registra una sesión activa en el daemon
 */
export async function registerActiveSession(
  sessionID,
  model = "unknown",
  provider = "unknown",
  turnID = null,
  clientInstanceId = CLIENT_INSTANCE_ID
) {
  activeSessions.set(sessionID, {
    model,
    provider,
    turnID,
    isActive: true,
    heartbeatTimer: null,
  });

  return sendControlRequest("/api/v1/sessions/register", "POST", {
    session_id: sessionID,
    client_type: "opencode",
    client_instance_id: clientInstanceId,
    turn_id: turnID,
    model,
    provider_id: provider,
    is_active: true,
  });
}

/**
 * Envía latido (heartbeat) de la sesión activa
 */
export async function sendSessionHeartbeat(
  sessionID,
  turnID = null,
  isActive = true,
  clientInstanceId = CLIENT_INSTANCE_ID
) {
  const existing = activeSessions.get(sessionID) || {};
  existing.turnID = turnID;
  existing.isActive = isActive;
  activeSessions.set(sessionID, existing);

  if (isActive) {
    startPeriodicHeartbeat(sessionID, turnID);
  } else {
    stopPeriodicHeartbeat(sessionID);
  }

  return sendControlRequest("/api/v1/sessions/heartbeat", "POST", {
    session_id: sessionID,
    client_instance_id: clientInstanceId,
    turn_id: turnID,
    is_active: isActive,
  });
}

/**
 * Desregistra una sesión al cerrarse
 */
export async function unregisterSession(sessionID, clientInstanceId = CLIENT_INSTANCE_ID) {
  stopPeriodicHeartbeat(sessionID);
  activeSessions.delete(sessionID);

  return sendControlRequest("/api/v1/sessions/unregister", "POST", {
    session_id: sessionID,
    client_instance_id: clientInstanceId,
  });
}

/**
 * Envía acuse de recibo de Stop hacia el daemon PeakGuard con command_id (T18.1.3)
 */
export async function sendStopAck(
  sessionID,
  transitionUtc,
  success,
  error = null,
  turnID = null,
  commandId = null,
  clientInstanceId = CLIENT_INSTANCE_ID
) {
  return sendControlRequest("/api/v1/adapters/stop-ack", "POST", {
    command_id: commandId || null,
    client_instance_id: clientInstanceId,
    session_id: sessionID,
    turn_id: turnID,
    transition_utc: transitionUtc || new Date().toISOString(),
    success: Boolean(success),
    error: error || null,
  });
}

// Variables de estado del suscriptor para reconexión y aislamiento (T18.1.6)
let reconnectAttempts = 0;
let reconnectTimer = null;
let stableTimer = null;
let isConnecting = false;
let currentSseReq = null;

/**
 * Conecta al stream de eventos SSE de PeakGuard para recibir órdenes de Stop nativo
 */
export function startEventSubscriber(client) {
  if (currentSseReq && !currentSseReq.destroyed && !currentSseReq.closed) {
    return currentSseReq;
  }
  isConnecting = true;

  const token = getAuthToken();
  const headers = {};
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const url = `${PEAKGUARD_CONTROL_URL}/api/v1/events`;

  function scheduleReconnect() {
    isConnecting = false;
    currentSseReq = null;
    if (stableTimer) {
      clearTimeout(stableTimer);
      stableTimer = null;
    }
    if (reconnectTimer) return;
    reconnectAttempts++;
    const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), 15000);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      startEventSubscriber(client);
    }, delay);
  }

  // Procesar orden Stop (T18.1.2, T18.1.3, T18.1.6, T18.2.2, T18.3.1)
  async function handleStopOrder(event) {
    const sessionID = event.session_id;

    // Validación de client_instance_id si viene especificado
    if (event.client_instance_id && event.client_instance_id !== CLIENT_INSTANCE_ID) {
      return;
    }

    // Aislamiento de sesión (T18.1.6):
    const currentSess = activeSessions.get(sessionID);
    if (!currentSess) {
      console.info(
        `[PeakGuard:OpenCode] Sesión '${sessionID}' no pertenece a este cliente OpenCode. Ignorando orden.`
      );
      return;
    }

    const turnID = typeof event.turn_id === "string" ? event.turn_id.trim() : null;
    const currentTurn = typeof currentSess.turnID === "string" ? currentSess.turnID.trim() : null;

    // 2. Si la sesión no tiene un turno en ejecución activa, descartar
    if (!currentSess.isActive) {
      console.info(
        `[PeakGuard:OpenCode] Sesión '${sessionID}' no tiene ningún turno activo en ejecución. Descartando Stop.`
      );
      await sendStopAck(
        sessionID,
        event.transition_utc,
        true,
        null,
        turnID || currentTurn,
        event.command_id,
        CLIENT_INSTANCE_ID
      );
      return;
    }

    // T18.3.1: Exigir coincidencia verificable entre instancia, sesión y turno antes de invocar un abort nativo.
    // Si falta event.turn_id o la identidad actual no puede comprobarse, no ejecutar un abort ambiguo.
    if (!turnID || !currentTurn) {
      const abortError = !turnID && !currentTurn
        ? "unverifiable_turn_identity: turno ausente en la orden y en la sesión"
        : !turnID
        ? "unverifiable_turn_identity: event.turn_id ausente en la orden de Stop"
        : "unverifiable_turn_identity: la sesión actual no tiene un turno activo comprobable";
      console.warn(
        `[PeakGuard:OpenCode] ${abortError}. Evitando abort ambiguo y aplicando fallback de gateway.`
      );
      await sendStopAck(
        sessionID,
        event.transition_utc,
        false,
        abortError,
        turnID || currentTurn,
        event.command_id,
        CLIENT_INSTANCE_ID
      );
      return;
    }

    // T18.2.2 & T18.3.1: Una orden antigua (turn_id anterior) NUNCA debe cancelar un turno nuevo
    if (turnID !== currentTurn) {
      console.warn(
        `[PeakGuard:OpenCode] Orden Stop antigua descartada: orden para '${turnID}', pero turno activo actual es '${currentTurn}'. No se aborta el turno nuevo.`
      );
      await sendStopAck(
        sessionID,
        event.transition_utc,
        false,
        `turn_mismatch: orden para '${turnID}', pero turno actual es '${currentTurn}'`,
        turnID,
        event.command_id,
        CLIENT_INSTANCE_ID
      );
      return;
    }

    // Deduplicar órdenes de Stop repetidas por command_id o clave compuesta
    const stopKey = event.command_id || `${sessionID}:${turnID}:${event.transition_utc || ""}`;
    if (processedStopKeys.has(stopKey)) {
      return;
    }
    processedStopKeys.add(stopKey);

    console.warn(
      `[PeakGuard:OpenCode] Recibida orden de Stop nativo válida para sesión '${sessionID}' (turno '${turnID}'). Abortando turno activo...`
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
          `[PeakGuard:OpenCode] Turno '${turnID}' abortado exitosamente mediante client.session.abort({ id: '${sessionID}' }).`
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

    // Enviar acuse con el estado REAL y command_id (T18.1.3, T18.2.3)
    await sendStopAck(
      sessionID,
      event.transition_utc,
      abortSuccess,
      abortError,
      turnID,
      event.command_id,
      CLIENT_INSTANCE_ID
    );
  }

  // Recuperar comandos pendientes perdidos durante desconexión (T18.1.3, T18.1.6)
  async function fetchPendingCommands() {
    try {
      const res = await sendControlRequest(
        `/api/v1/adapters/commands/pending?client_instance_id=${encodeURIComponent(CLIENT_INSTANCE_ID)}`,
        "GET"
      );
      if (res.statusCode === 200 && Array.isArray(res.body?.commands)) {
        for (const cmd of res.body.commands) {
          await handleStopOrder(cmd);
        }
      }
    } catch (err) {
      console.error("[PeakGuard:OpenCode] Error recuperando comandos pendientes:", err);
    }
  }

  const req = http.get(url, { headers }, (res) => {
    if (res.statusCode !== 200) {
      console.warn(`[PeakGuard:OpenCode] SSE rechazado por Control API (Status ${res.statusCode}). Reintentando con backoff...`);
      scheduleReconnect();
      return;
    }

    // Restablecer contador de reintentos solo después de 5s de conexión estable (T18.1.6)
    stableTimer = setTimeout(() => {
      reconnectAttempts = 0;
    }, 5000);

    // Recuperar comandos pendientes tras conexión
    fetchPendingCommands();

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
              await handleStopOrder(event);
            }
          } catch {
            // Ignorar líneas no JSON o comentarios keepalive
          }
        }
      }
    });

    res.on("end", () => {
      scheduleReconnect();
    });
  });

  req.on("error", () => {
    scheduleReconnect();
  });

  req.on("close", () => {
    if (currentSseReq === req) {
      currentSseReq = null;
      isConnecting = false;
    }
  });

  currentSseReq = req;
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
      const model = properties?.info?.model || properties?.model || "unknown";
      let provider = properties?.provider || properties?.info?.provider || "unknown";
      if (provider === "unknown" && typeof model === "string" && model.toLowerCase().includes("deepseek")) {
        provider = "deepseek_direct";
      }

      if (type === "session.created" && sessionID) {
        await registerActiveSession(sessionID, model, provider);
      }

      if (type === "message.created" && sessionID) {
        await sendSessionHeartbeat(sessionID, properties?.messageId, true);
      }

      if ((type === "message.completed" || type === "session.idle") && sessionID) {
        await sendSessionHeartbeat(sessionID, properties?.messageId, false);
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
