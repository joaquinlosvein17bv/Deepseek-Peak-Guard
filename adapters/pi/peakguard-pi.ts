/**
 * PeakGuard Extension para Pi Coding Agent (0.87.1+)
 * Ubicación recomendada: ~/.pi/agent/extensions/peakguard.ts
 *
 * Implementa la integración real conforme a la API de extensiones de Pi:
 * - Autenticación con Bearer Token desde entorno o ~/.peakguard/control.token
 * - Identidad compuesta con CLIENT_INSTANCE_ID estable por proceso (T18.1.2)
 * - Registra callbacks con pi.on("session_start"), pi.on("agent_start"), pi.on("turn_start").
 * - Registra sesiones activas en el demonio central de PeakGuard sin asumir deepseek-chat por defecto (T18.1.1).
 * - Latidos periódicos durante turnos activos para evitar poda en razonamientos largos (T18.1.4).
 * - Recuperación de órdenes Stop pendientes al conectarse o reconectarse (T18.1.3).
 * - Indicador UI que solo reporta "Protegido" cuando la cobertura verificada es PROTECTED (T18.1.1).
 * - Retiene el contexto activo del turno para invocar ctx.abort() o pi.abort() sin matar el proceso.
 * - Reporta acuses de recibo (Stop ACK) distinguiendo éxitos de fallos reales (sin falsos positivos).
 */

import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import crypto from "node:crypto";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";
const SOURCE = "peakguard:pi";

// Identificador único y estable por instancia de proceso (T18.1.2)
export const CLIENT_INSTANCE_ID = `pi_inst_${process.pid}_${crypto.randomBytes(6).toString("hex")}`;

let cachedToken: string | null = null;

export function getAuthToken(): string | null {
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
    // Continuar sin token si falla la lectura
  }
  return null;
}

export async function sendControlRequest(
  endpoint: string,
  method = "GET",
  body: any = null
): Promise<{ statusCode: number; body?: any; error?: any }> {
  const token = getAuthToken();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const payload = body ? JSON.stringify(body) : null;
  if (payload) {
    headers["Content-Length"] = String(Buffer.byteLength(payload));
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
              statusCode: res.statusCode || 0,
              body: data ? JSON.parse(data) : null,
            });
          } catch {
            resolve({ statusCode: res.statusCode || 0, body: data });
          }
        });
      }
    );
    req.on("error", (err) => resolve({ statusCode: 0, error: err }));
    if (payload) req.write(payload);
    req.end();
  });
}

export async function getPeakGuardStatus(): Promise<any> {
  const res = await sendControlRequest("/api/v1/status", "GET");
  return res.statusCode === 200 ? res.body : null;
}

export function formatPiStatusLabel(status: any): string {
  if (!status) {
    return "○ DESCONECTADO";
  }
  if (status.is_peak) {
    // T18.1.1: Solo indicar (Protegido) si status.coverage === "PROTECTED".
    // Si la cobertura es PARTIAL, UNVERIFIED o UNCOVERED, mostrar el estado real.
    const cov = status.coverage || "UNVERIFIED";
    return cov === "PROTECTED" ? "⛔ PEAK (Protegido)" : `⛔ PEAK (${cov})`;
  }
  return `🛡️ OFF-PEAK (${status.seconds_to_peak ? Math.floor(status.seconds_to_peak / 60) + "m" : "--"})`;
}

export async function registerActiveSession(
  sessionID: string,
  turnID: string | null = null,
  model: any = "unknown",
  clientInstanceId = CLIENT_INSTANCE_ID
): Promise<boolean> {
  activeSessionId = sessionID;
  if (turnID) {
    activeTurnId = turnID;
  }
  const modelStr = typeof model === "string" ? model : (model?.id || "unknown");
  const isDeepSeek = Boolean(modelStr && (modelStr.toLowerCase().includes("deepseek") || modelStr.toLowerCase().startsWith("deepseek")));
  const provider_id = isDeepSeek ? "deepseek_direct" : (modelStr === "unknown" ? "unknown" : "other");
  const res = await sendControlRequest("/api/v1/sessions/register", "POST", {
    session_id: sessionID,
    client_type: "pi",
    client_instance_id: clientInstanceId,
    turn_id: turnID,
    model: modelStr,
    provider_id,
    is_active: true,
  });
  return res.statusCode === 200;
}

export async function sendStopAck(
  sessionID: string,
  transitionUtc: string,
  success: boolean,
  error?: string,
  turnID?: string | null,
  commandId?: string | null,
  clientInstanceId = CLIENT_INSTANCE_ID
): Promise<boolean> {
  const res = await sendControlRequest("/api/v1/adapters/stop-ack", "POST", {
    command_id: commandId || null,
    client_instance_id: clientInstanceId,
    session_id: sessionID,
    turn_id: turnID || null,
    transition_utc: transitionUtc || new Date().toISOString(),
    success: Boolean(success),
    error: error || null,
  });
  return res.statusCode === 200 || res.statusCode === 202;
}

export let activeCtx: any = null;
export let activeSessionId: string = "pi-default-session";
export let activeTurnId: string | null = null;
export let currentModel = "unknown";
export let activePiAgent: any = null;
export const processedStopKeys = new Set<string>();

// Procesar orden Stop de manera centralizada (SSE o recuperación REST)
export async function handleStopOrder(event: any, ctx: any = activeCtx, pi: any = activePiAgent) {
  const targetSession = event.session_id;

  // Validación estricta de instancia y sesión (T18.1.2)
  if (event.client_instance_id && event.client_instance_id !== CLIENT_INSTANCE_ID) {
    return;
  }
  if (!targetSession || targetSession !== activeSessionId) {
    return;
  }

  const eventTurn = typeof event.turn_id === "string" ? event.turn_id.trim() : null;
  const currentTurn = typeof activeTurnId === "string" ? activeTurnId.trim() : null;

  // T18.3.1: Exigir coincidencia verificable entre instancia, sesión y turno antes de invocar abort nativo.
  // Si falta event.turn_id o la identidad actual no puede comprobarse, no ejecutar un abort ambiguo.
  if (!eventTurn || !currentTurn) {
    const reason = !eventTurn && !currentTurn
      ? "unverifiable_turn_identity: turno ausente en la orden y en la sesión"
      : !eventTurn
      ? "unverifiable_turn_identity: event.turn_id ausente en la orden de Stop"
      : "unverifiable_turn_identity: la sesión actual no tiene un turno activo comprobable";
    console.warn(`[PeakGuard:Pi] ${reason}. Se omite abort ambiguo y se mantiene bloqueo en gateway.`);
    await sendStopAck(
      targetSession,
      event.transition_utc,
      false,
      reason,
      eventTurn || currentTurn,
      event.command_id,
      CLIENT_INSTANCE_ID
    );
    return;
  }

  // T18.2.2 & T18.3.1: Una orden antigua (turn_id anterior) NUNCA debe cancelar un turno nuevo
  if (eventTurn !== currentTurn) {
    console.warn(
      `[PeakGuard:Pi] Orden Stop descartada por discordancia de turno: orden para '${eventTurn}', pero turno actual activo es '${currentTurn}'.`
    );
    await sendStopAck(
      targetSession,
      event.transition_utc,
      false,
      "turn_mismatch",
      eventTurn,
      event.command_id,
      CLIENT_INSTANCE_ID
    );
    return;
  }

  // Deduplicar órdenes de Stop repetidas por command_id o clave compuesta
  const stopKey = event.command_id || `${targetSession}:${eventTurn}:${event.transition_utc || ""}`;
  if (processedStopKeys.has(stopKey)) {
    return;
  }
  processedStopKeys.add(stopKey);

  console.warn(
    `[PeakGuard:Pi] Orden de Stop recibida para '${targetSession}'. Abortando turno activo...`
  );

  let success = false;
  let errorMsg: string | undefined;

  // Invocación nativa sin matar proceso (Principio P1)
  try {
    const abortTarget = ctx || activeCtx;
    const piTarget = pi || activePiAgent;
    if (abortTarget && typeof abortTarget.abort === "function") {
      abortTarget.abort();
      success = true;
      console.info("[PeakGuard:Pi] activeCtx.abort() ejecutado con éxito.");
    } else if (typeof piTarget?.abort === "function") {
      piTarget.abort();
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

  // Enviar acuse con status REAL y command_id para entrega garantizada (T18.1.3)
  await sendStopAck(
    targetSession,
    event.transition_utc,
    success,
    errorMsg,
    event.turn_id,
    event.command_id,
    CLIENT_INSTANCE_ID
  );
}

export default function peakguardExtension(pi: any) {
  activePiAgent = pi;
  let sseReq: http.ClientRequest | null = null;
  let reconnectTimer: NodeJS.Timeout | null = null;
  let reconnectAttempts = 0;
  let heartbeatTimer: NodeJS.Timeout | null = null;

  function startHeartbeat() {
    stopHeartbeat();
    heartbeatTimer = setInterval(async () => {
      if (activeTurnId) {
        await sendControlRequest("/api/v1/sessions/heartbeat", "POST", {
          session_id: activeSessionId,
          client_instance_id: CLIENT_INSTANCE_ID,
          turn_id: activeTurnId,
          is_active: true,
        });
      }
    }, 10000);
  }

  function stopHeartbeat() {
    if (heartbeatTimer) {
      clearInterval(heartbeatTimer);
      heartbeatTimer = null;
    }
  }

  // 1. Mantener referencia al contexto de sesión y turno activo
  if (typeof pi?.on === "function") {
    pi.on("session_start", (_event: any, ctx: any) => {
      activeCtx = ctx;
      try {
        if (ctx?.sessionManager?.getSessionId) {
          activeSessionId = ctx.sessionManager.getSessionId() || activeSessionId;
        } else if (ctx?.sessionId) {
          activeSessionId = ctx.sessionId;
        }
      } catch {}
      if (ctx?.model) {
        currentModel = typeof ctx.model === "string" ? ctx.model : (ctx.model?.id || "unknown");
      }
      updateStatusDisplay();
    });

    pi.on("agent_start", (_event: any, ctx: any) => {
      activeCtx = ctx;
      try {
        if (ctx?.sessionManager?.getSessionId) {
          activeSessionId = ctx.sessionManager.getSessionId() || activeSessionId;
        } else if (ctx?.sessionId) {
          activeSessionId = ctx.sessionId;
        }
      } catch {}
      if (ctx?.model) {
        currentModel = typeof ctx.model === "string" ? ctx.model : (ctx.model?.id || "unknown");
      }
      updateStatusDisplay();
    });

    pi.on("turn_start", async (event: any, ctx: any) => {
      activeCtx = ctx;
      try {
        if (ctx?.sessionManager?.getSessionId) {
          activeSessionId = ctx.sessionManager.getSessionId() || activeSessionId;
        } else if (ctx?.sessionId) {
          activeSessionId = ctx.sessionId;
        }
      } catch {}

      if (event?.turnIndex !== undefined) {
        activeTurnId = `turn-${event.turnIndex}`;
      } else if (ctx?.turnId) {
        activeTurnId = ctx.turnId;
      } else if (!activeTurnId) {
        activeTurnId = `turn-${Date.now()}`;
      }

      if (ctx?.model) {
        currentModel = typeof ctx.model === "string" ? ctx.model : (ctx.model?.id || "unknown");
      }

      console.info(`[PeakGuard:Pi] turn_start: session='${activeSessionId}', turn='${activeTurnId}', model='${currentModel}'`);
      await registerActiveSession(activeSessionId, activeTurnId, currentModel);
      startHeartbeat();
    });

    pi.on("agent_settled", async (_event: any, ctx: any) => {
      if (activeCtx === ctx || !ctx) {
        stopHeartbeat();
        await sendControlRequest("/api/v1/sessions/heartbeat", "POST", {
          session_id: activeSessionId,
          client_instance_id: CLIENT_INSTANCE_ID,
          turn_id: activeTurnId,
          is_active: false,
        });
        activeTurnId = null;
      }
    });
  }

  // 2. Actualizar indicador en UI con verificación rigurosa de cobertura (T18.1.1)
  async function updateStatusDisplay() {
    try {
      const status = await getPeakGuardStatus();
      const label = formatPiStatusLabel(status);

      try {
        if (activeCtx?.hasUI && activeCtx?.ui?.setStatus) {
          activeCtx.ui.setStatus("peakguard", label);
        } else if (pi?.ui?.setStatus) {
          pi.ui.setStatus("peakguard", label);
        }
      } catch {
        // UI puede no estar disponible en modo headless o CLI
      }
    } catch {
      // Ignorar fallos de red al actualizar estado
    }
  }

  updateStatusDisplay();
  const pollInterval = setInterval(updateStatusDisplay, 15000);

  let isConnecting = false;
  let stableTimer: NodeJS.Timeout | null = null;

  function clearStableTimer() {
    if (stableTimer) {
      clearTimeout(stableTimer);
      stableTimer = null;
    }
  }

  function scheduleReconnect() {
    clearStableTimer();
    isConnecting = false;
    if (sseReq && !sseReq.destroyed) {
      try {
        sseReq.destroy();
      } catch {
        // Ignorar
      }
      sseReq = null;
    }
    if (reconnectTimer) return;
    reconnectAttempts++;
    const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), 15000);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connectEvents();
    }, delay);
  }

  // Recuperar comandos pendientes perdidos durante desconexión (T18.1.3)
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
      console.error("[PeakGuard:Pi] Error recuperando comandos pendientes:", err);
    }
  }

  // 3. Conectar a eventos SSE del Control API de PeakGuard
  function connectEvents() {
    if (isConnecting) return;
    isConnecting = true;

    if (sseReq && !sseReq.destroyed) {
      try {
        sseReq.destroy();
      } catch {
        // Ignorar
      }
      sseReq = null;
    }

    const token = getAuthToken();
    const headers: Record<string, string> = {};
    if (token) headers["Authorization"] = `Bearer ${token}`;

    const url = `${PEAKGUARD_CONTROL_URL}/api/v1/events`;

    sseReq = http.get(url, { headers }, (res) => {
      if (res.statusCode !== 200) {
        scheduleReconnect();
        return;
      }

      isConnecting = false;

      // T18.2.5: No resetear reconnectAttempts inmediatamente tras HTTP 200.
      // Solo resetear el contador de reintentos tras 5 segundos de conexión estable ininterrumpida.
      clearStableTimer();
      stableTimer = setTimeout(() => {
        reconnectAttempts = 0;
        stableTimer = null;
      }, 5000);

      // Recuperar comandos pendientes al conectar/reconectar
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
              if (event.type === "stop_requested") {
                await handleStopOrder(event);
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
        scheduleReconnect();
      });

      res.on("close", () => {
        scheduleReconnect();
      });
    });

    sseReq.on("error", () => {
      scheduleReconnect();
    });
  }

  connectEvents();

  return () => {
    clearStableTimer();
    stopHeartbeat();
    clearInterval(pollInterval);
    if (reconnectTimer) clearTimeout(reconnectTimer);
    if (sseReq && !sseReq.destroyed) sseReq.destroy();
  };
}
