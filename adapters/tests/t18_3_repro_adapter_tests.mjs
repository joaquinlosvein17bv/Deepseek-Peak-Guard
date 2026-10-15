/**
 * Reproducción de defectos en adaptadores para T18.3:
 * T18.3.1 — Validación estricta de Stop:
 * - Caso 1: ambos identificadores presentes y coincidentes -> abort OK
 * - Caso 2: event.turn_id ausente pero turno activo presente -> STOP AMBIGUO, NO ABORTAR
 * - Caso 3: event.turn_id presente pero turno activo ausente -> STOP AMBIGUO, NO ABORTAR
 * - Caso 4: ambos identificadores ausentes -> STOP AMBIGUO, NO ABORTAR
 * - Caso 5: identificadores discordantes (orden antigua vs turno nuevo) -> NO ABORTAR
 */

import http from "node:http";
import assert from "node:assert";

console.log("=== Reproducción de defectos T18.3 en adaptadores ===");

let receivedAcks = [];
let sseSubscribers = [];

const mockServer = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  if (url.pathname === "/api/v1/sessions/register" || url.pathname === "/api/v1/sessions/heartbeat") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "ok" }));
    return;
  }
  if (url.pathname === "/api/v1/adapters/stop-ack") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      receivedAcks.push(JSON.parse(body));
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "acknowledged" }));
    });
    return;
  }
  if (url.pathname === "/api/v1/adapters/commands/pending") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ commands: [] }));
    return;
  }
  if (url.pathname === "/api/v1/events") {
    res.writeHead(200, { "Content-Type": "text/event-stream" });
    res.write("data: {\"type\":\"connected\"}\n\n");
    sseSubscribers.push(res);
    return;
  }
  res.writeHead(404);
  res.end();
});

mockServer.listen(0, "127.0.0.1", async () => {
  const port = mockServer.address().port;
  process.env.PEAKGUARD_CONTROL_URL = `http://127.0.0.1:${port}`;
  process.env.PEAKGUARD_CONTROL_TOKEN = "tok-18-3";

  // ==========================================
  // PARTE 1: PRUEBAS EN OPENCODE ADAPTER
  // ==========================================
  console.log("--- 1. Pruebas de Stop Estricto en OpenCode ---");
  const {
    registerActiveSession,
    sendSessionHeartbeat,
    startEventSubscriber,
    CLIENT_INSTANCE_ID: OC_INSTANCE_ID,
  } = await import("../opencode/peakguard-opencode-server.js");

  let abortCalls = [];
  const mockClient = {
    session: {
      abort: async ({ path }) => {
        abortCalls.push(path.id);
        return { aborted: true };
      },
    },
  };

  const ocSse = startEventSubscriber(mockClient);
  await new Promise((r) => setTimeout(r, 200));

  // Configurar sesión activa con turn-active
  const sessionID = "sess-oc-strict-turn";
  await registerActiveSession(sessionID, "deepseek-chat", "deepseek_direct", "turn-active");

  // CASO 2: event.turn_id AUSENTE, pero turno activo presente en cliente ("turn-active")
  // DEBE RECHAZARSE COMO AMBIGUO SIN INVOCAR ABORT
  abortCalls = [];
  receivedAcks = [];
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        command_id: "cmd_missing_event_turn",
        client_instance_id: OC_INSTANCE_ID,
        session_id: sessionID,
        turn_id: null, // AUSENTE
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }
  await new Promise((r) => setTimeout(r, 200));
  assert.strictEqual(
    abortCalls.length,
    0,
    "[OpenCode] Si event.turn_id está ausente, NO se debe abortar la sesión (Stop ambiguo)"
  );
  assert.strictEqual(receivedAcks.length, 1);
  assert.strictEqual(receivedAcks[0].success, false, "Stop ambiguo debe reportar success=false");
  console.log("  ✓ [OpenCode] Caso 2 (event.turn_id ausente): abort prevenido y Stop no confirmado.");

  // CASO 3: event.turn_id PRESENTE ("turn-explicit"), pero cliente no tiene turno activo (null)
  // DEBE RECHAZARSE COMO AMBIGUO SIN INVOCAR ABORT
  await sendSessionHeartbeat(sessionID, null, true); // eliminar turnID activo
  abortCalls = [];
  receivedAcks = [];
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        command_id: "cmd_missing_client_turn",
        client_instance_id: OC_INSTANCE_ID,
        session_id: sessionID,
        turn_id: "turn-explicit",
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }
  await new Promise((r) => setTimeout(r, 200));
  assert.strictEqual(
    abortCalls.length,
    0,
    "[OpenCode] Si cliente no tiene turno comprobable, NO se debe abortar la sesión"
  );
  assert.strictEqual(receivedAcks[0].success, false);
  console.log("  ✓ [OpenCode] Caso 3 (turno cliente ausente): abort prevenido y Stop no confirmado.");

  // CASO 4: AMBOS IDENTIFICADORES AUSENTES
  abortCalls = [];
  receivedAcks = [];
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        command_id: "cmd_both_missing",
        client_instance_id: OC_INSTANCE_ID,
        session_id: sessionID,
        turn_id: null,
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }
  await new Promise((r) => setTimeout(r, 200));
  assert.strictEqual(abortCalls.length, 0, "[OpenCode] Ambos turnos ausentes: NO abortar");
  assert.strictEqual(receivedAcks[0].success, false);
  console.log("  ✓ [OpenCode] Caso 4 (ambos ausentes): abort prevenido y Stop no confirmado.");

  // CASO 5: IDENTIFICADORES DISCORDANTES (orden para 'turn-old' vs turno actual 'turn-new')
  await sendSessionHeartbeat(sessionID, "turn-new", true);
  abortCalls = [];
  receivedAcks = [];
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        command_id: "cmd_mismatch",
        client_instance_id: OC_INSTANCE_ID,
        session_id: sessionID,
        turn_id: "turn-old",
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }
  await new Promise((r) => setTimeout(r, 200));
  assert.strictEqual(abortCalls.length, 0, "[OpenCode] Turno discordante: NO abortar turno nuevo");
  assert.strictEqual(receivedAcks[0].success, false);
  console.log("  ✓ [OpenCode] Caso 5 (discordancia de turno): abort prevenido.");

  // CASO 1: AMBOS PRESENTES Y COINCIDENTES ('turn-new' === 'turn-new') -> ABORT EXITOSO
  abortCalls = [];
  receivedAcks = [];
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        command_id: "cmd_matching_turn",
        client_instance_id: OC_INSTANCE_ID,
        session_id: sessionID,
        turn_id: "turn-new",
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }
  await new Promise((r) => setTimeout(r, 200));
  assert.strictEqual(abortCalls.length, 1, "[OpenCode] Turnos coincidentes: DEBE abortar");
  assert.strictEqual(receivedAcks[0].success, true, "Stop confirmado con success=true");
  console.log("  ✓ [OpenCode] Caso 1 (coincidencia exacta): abort ejecutado con éxito.");

  ocSse.destroy();

  // ==========================================
  // PARTE 2: PRUEBAS EN PI ADAPTER
  // ==========================================
  console.log("--- 2. Pruebas de Stop Estricto en Pi ---");
  const {
    default: peakguardPiExtension,
    CLIENT_INSTANCE_ID: PI_INSTANCE_ID,
  } = await import("../pi/peakguard-pi.ts");

  let piAbortCalls = 0;
  let piEventListeners = {};
  const mockPi = {
    on: (evt, cb) => {
      piEventListeners[evt] = cb;
    },
    abort: () => {
      piAbortCalls++;
    },
  };

  const cleanupPi = peakguardPiExtension(mockPi);
  const piCtx = {
    sessionId: "pi-sess-strict",
    turnId: "pi-turn-1",
    model: "deepseek-chat",
    abort: () => {
      piAbortCalls++;
    },
  };

  // Iniciar turn-1
  await piEventListeners["turn_start"]({}, piCtx);
  await new Promise((r) => setTimeout(r, 200));

  // Pi CASO 2: event.turn_id AUSENTE con activeTurnId="pi-turn-1" -> NO ABORTAR
  piAbortCalls = 0;
  receivedAcks = [];
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        command_id: "cmd_pi_missing_turn",
        client_instance_id: PI_INSTANCE_ID,
        session_id: "pi-sess-strict",
        turn_id: null,
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }
  await new Promise((r) => setTimeout(r, 200));
  assert.strictEqual(piAbortCalls, 0, "[Pi] Si event.turn_id está ausente: NO abortar");
  assert.strictEqual(receivedAcks[0].success, false);
  console.log("  ✓ [Pi] Caso 2 (event.turn_id ausente): abort prevenido.");

  // Pi CASO 1: Coincidencia exacta -> ABORT EXITOSO
  piAbortCalls = 0;
  receivedAcks = [];
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        command_id: "cmd_pi_match",
        client_instance_id: PI_INSTANCE_ID,
        session_id: "pi-sess-strict",
        turn_id: "pi-turn-1",
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }
  await new Promise((r) => setTimeout(r, 200));
  assert.strictEqual(piAbortCalls, 1, "[Pi] Coincidencia exacta: DEBE abortar");
  assert.strictEqual(receivedAcks[0].success, true);
  console.log("  ✓ [Pi] Caso 1 (coincidencia exacta): abort ejecutado con éxito.");

  cleanupPi();
  mockServer.close();
  console.log("=== Todas las pruebas de reproducción T18.3 en adaptadores pasaron ===");
});
