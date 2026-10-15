/**
 * PeakGuard Adapter Mock Verification Suite (T18.1)
 * Valida el comportamiento de los adaptadores de OpenCode y Pi en Node.js
 * contra un servidor Control API simulado:
 * - Identidad compuesta con client_instance_id (T18.1.2)
 * - Indicador UI de Pi que jamás afirma "Protegido" si coverage es PARTIAL o UNVERIFIED (T18.1.1)
 * - Recuperación de órdenes Stop pendientes al conectarse (T18.1.3)
 * - Aislamiento estricto de sesiones entre instancias (T18.1.6)
 * - Latidos periódicos y ciclo de vida de razonamiento (T18.1.4)
 */

import http from "node:http";
import assert from "node:assert";

// 1. Iniciar servidor simulado de Control API
const TEST_TOKEN = "mock-secret-token-xyz";
let receivedAcks = [];
let registeredSessions = [];
let unregisterCalls = [];
let heartbeatCalls = [];
let sseSubscribers = [];
let mockPendingCommands = [];

const server = http.createServer((req, res) => {
  const authHeader = req.headers["authorization"];
  const url = new URL(req.url, `http://${req.headers.host}`);

  // En T18 la autenticación requiere estricto Bearer Token en headers (sin query param en URL)
  const isAuthorized = authHeader === `Bearer ${TEST_TOKEN}`;

  if (url.pathname === "/api/v1/status") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(
      JSON.stringify({
        mode: "restricted_stop",
        is_peak: false,
        coverage: "PROTECTED",
        seconds_to_peak: 1800,
      })
    );
    return;
  }

  if (!isAuthorized) {
    res.writeHead(401, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: "Unauthorized" }));
    return;
  }

  if (url.pathname === "/api/v1/adapters/commands/pending" && req.method === "GET") {
    const instId = url.searchParams.get("client_instance_id");
    const cmds = mockPendingCommands.filter((c) => !instId || c.client_instance_id === instId);
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ client_instance_id: instId, commands: cmds }));
    return;
  }

  if (url.pathname === "/api/v1/sessions/register" && req.method === "POST") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const parsed = JSON.parse(body);
      registeredSessions.push(parsed);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "registered" }));
    });
    return;
  }

  if (url.pathname === "/api/v1/sessions/heartbeat" && req.method === "POST") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const parsed = JSON.parse(body);
      heartbeatCalls.push(parsed);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "heartbeat_received" }));
    });
    return;
  }

  if (url.pathname === "/api/v1/sessions/unregister" && req.method === "POST") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const parsed = JSON.parse(body);
      unregisterCalls.push(parsed);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "unregistered" }));
    });
    return;
  }

  if (url.pathname === "/api/v1/adapters/stop-ack" && req.method === "POST") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const parsed = JSON.parse(body);
      receivedAcks.push(parsed);
      res.writeHead(parsed.success ? 200 : 202, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: parsed.success ? "acknowledged" : "failed" }));
    });
    return;
  }

  if (url.pathname === "/api/v1/events") {
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    });
    res.write("data: {\"type\":\"connected\"}\n\n");
    sseSubscribers.push(res);
    return;
  }

  res.writeHead(404);
  res.end();
});

server.listen(0, "127.0.0.1", async () => {
  const port = server.address().port;
  process.env.PEAKGUARD_CONTROL_URL = `http://127.0.0.1:${port}`;
  process.env.PEAKGUARD_CONTROL_TOKEN = TEST_TOKEN;

  console.log(`[TEST] Mock Control API escuchando en el puerto ${port}`);

  try {
    // Importar adaptadores dinámicamente con las variables de entorno configuradas
    const {
      registerActiveSession,
      sendStopAck,
      startEventSubscriber,
      CLIENT_INSTANCE_ID: OPENCODE_INSTANCE_ID,
    } = await import("../opencode/peakguard-opencode-server.js");

    const { fetchStatus } = await import("../opencode/peakguard-tui-sidebar.js");
    const {
      default: peakguardPiExtension,
      formatPiStatusLabel,
      CLIENT_INSTANCE_ID: PI_INSTANCE_ID,
    } = await import("../pi/peakguard-pi.ts");

    console.log("--- 1. Probando fetchStatus() en Widget TUI ---");
    const status = await fetchStatus();
    assert.strictEqual(status.coverage, "PROTECTED");
    assert.strictEqual(status.mode, "restricted_stop");
    console.log("  ✓ Widget TUI sincronizó estado correctamente.");

    console.log("--- 2. Probando formateo de indicador Pi (T18.1.1) ---");
    const labelDisconnected = formatPiStatusLabel(null);
    assert.strictEqual(labelDisconnected, "○ DESCONECTADO");

    const labelPartial = formatPiStatusLabel({ is_peak: true, coverage: "PARTIAL" });
    assert.strictEqual(labelPartial, "⛔ PEAK (PARTIAL)");
    assert.ok(!labelPartial.includes("Protegido"), "No debe reportar 'Protegido' si coverage es PARTIAL");

    const labelUnverified = formatPiStatusLabel({ is_peak: true, coverage: "UNVERIFIED" });
    assert.strictEqual(labelUnverified, "⛔ PEAK (UNVERIFIED)");

    const labelProtected = formatPiStatusLabel({ is_peak: true, coverage: "PROTECTED" });
    assert.strictEqual(labelProtected, "⛔ PEAK (Protegido)");
    console.log("  ✓ Indicador de Pi refleja fielmente la cobertura sin falsos 'Protegido'.");

    console.log("--- 3. Probando registro de sesiones compuestas (T18.1.2) ---");
    assert.ok(OPENCODE_INSTANCE_ID.startsWith("opencode_inst_"), "OpenCode debe tener client_instance_id válido");
    assert.ok(PI_INSTANCE_ID.startsWith("pi_inst_"), "Pi debe tener client_instance_id válido");

    const regRes = await registerActiveSession("session-test-1", "deepseek-chat", "deepseek_direct", "turn-1");
    assert.strictEqual(regRes.statusCode, 200);
    assert.strictEqual(registeredSessions.length, 1);
    assert.strictEqual(registeredSessions[0].session_id, "session-test-1");
    assert.strictEqual(registeredSessions[0].client_instance_id, OPENCODE_INSTANCE_ID);
    console.log("  ✓ Sesión registrada con identidad compuesta (session_id + client_instance_id).");

    console.log("--- 4. Probando Stop nativo exitoso en OpenCode (T17.2, T17.6, T18.1.3) ---");
    let abortCalled = false;
    let abortTarget = null;

    const mockClient = {
      session: {
        abort: async ({ path }) => {
          abortCalled = true;
          abortTarget = path.id;
          return { aborted: true };
        },
      },
    };

    const sseReq = startEventSubscriber(mockClient);

    // Esperar conexión SSE
    await new Promise((r) => setTimeout(r, 200));

    // Emitir evento stop_requested desde el mock server
    for (const sub of sseSubscribers) {
      sub.write(
        `data: ${JSON.stringify({
          type: "stop_requested",
          command_id: "cmd_stop_001",
          client_instance_id: OPENCODE_INSTANCE_ID,
          session_id: "session-test-1",
          turn_id: "turn-1",
          transition_utc: "2026-10-15T01:00:00Z",
        })}\n\n`
      );
    }

    // Esperar procesamiento y acuse de Stop
    await new Promise((r) => setTimeout(r, 300));

    assert.strictEqual(abortCalled, true, "client.session.abort debe ser invocado");
    assert.strictEqual(abortTarget, "session-test-1");
    assert.strictEqual(receivedAcks.length, 1);
    assert.strictEqual(receivedAcks[0].success, true, "Stop exitoso debe reportar success=true");
    assert.strictEqual(receivedAcks[0].command_id, "cmd_stop_001", "Stop ACK debe incluir command_id");
    assert.strictEqual(receivedAcks[0].client_instance_id, OPENCODE_INSTANCE_ID);
    console.log("  ✓ client.session.abort() ejecutado y confirmado con Stop ACK exitoso.");

    sseReq.destroy();
    sseSubscribers = [];
    receivedAcks = [];

    console.log("--- 5. Probando Stop con fallo: NUNCA reportar falso positivo (T17.1, T17.6) ---");
    await registerActiveSession("session-failing-1", "deepseek-chat", "deepseek_direct", "turn-fail");

    const failingClient = {
      session: {
        abort: async () => {
          throw new Error("Simulated editor crash during abort");
        },
      },
    };

    const sseReq2 = startEventSubscriber(failingClient);
    await new Promise((r) => setTimeout(r, 200));

    for (const sub of sseSubscribers) {
      sub.write(
        `data: ${JSON.stringify({
          type: "stop_requested",
          command_id: "cmd_stop_002",
          client_instance_id: OPENCODE_INSTANCE_ID,
          session_id: "session-failing-1",
          transition_utc: "2026-10-15T01:00:00Z",
        })}\n\n`
      );
    }

    await new Promise((r) => setTimeout(r, 300));

    const failAck = receivedAcks.find((a) => a.session_id === "session-failing-1");
    assert.ok(failAck, "Se debe emitir Stop ACK incluso ante fallo de abort");
    assert.strictEqual(failAck.success, false, "Fallo de abort DEBE reportar success=false");
    assert.ok(failAck.error.includes("Simulated editor crash"), "Error reportado debe contener el mensaje real");
    console.log("  ✓ Fallo de abort reporta estrictamente success=false sin falsos positivos.");

    sseReq2.destroy();
    sseSubscribers = [];
    receivedAcks = [];

    console.log("--- 6. Probando aislamiento de sesión en OpenCode (ignora sesiones ajenas) ---");
    let foreignAbortCalled = false;
    const isolationClient = {
      session: {
        abort: async () => {
          foreignAbortCalled = true;
          return { aborted: true };
        },
      },
    };

    const sseReq3 = startEventSubscriber(isolationClient);
    await new Promise((r) => setTimeout(r, 200));

    for (const sub of sseSubscribers) {
      sub.write(
        `data: ${JSON.stringify({
          type: "stop_requested",
          session_id: "session-foreign-unknown-999",
          transition_utc: "2026-10-15T01:00:00Z",
        })}\n\n`
      );
    }

    await new Promise((r) => setTimeout(r, 300));
    assert.strictEqual(foreignAbortCalled, false, "No debe abortar sesiones que no pertenecen a este cliente");
    console.log("  ✓ OpenCode ignora Stop de sesiones ajenas no registradas en su contexto.");

    sseReq3.destroy();
    sseSubscribers = [];
    receivedAcks = [];

    console.log("--- 7. Probando recuperación de órdenes Stop pendientes al conectar (T18.1.3) ---");
    // Registrar una sesión en OpenCode que sufrirá un corte de red durante la emisión de Stop
    await registerActiveSession("session-pending-recovery-1", "deepseek-chat", "deepseek_direct", "turn-p1");

    // El servidor tiene una orden pendiente encolada para este client_instance_id
    mockPendingCommands.push({
      command_id: "cmd_missed_during_disconnect_999",
      client_type: "opencode",
      client_instance_id: OPENCODE_INSTANCE_ID,
      session_id: "session-pending-recovery-1",
      turn_id: "turn-p1",
      transition_utc: "2026-10-15T01:00:00Z",
      reason: "Peak transitado durante desconexión",
    });

    let recoveredAbortCalled = false;
    const recoveryClient = {
      session: {
        abort: async ({ path }) => {
          if (path.id === "session-pending-recovery-1") {
            recoveredAbortCalled = true;
          }
          return { aborted: true };
        },
      },
    };

    // Conectar el suscriptor: debe consultar GET /api/v1/adapters/commands/pending y procesarla
    const sseReqRecov = startEventSubscriber(recoveryClient);
    await new Promise((r) => setTimeout(r, 400));

    assert.strictEqual(recoveredAbortCalled, true, "Orden pendiente debe ser ejecutada tras reconexión");
    const recovAck = receivedAcks.find((a) => a.command_id === "cmd_missed_during_disconnect_999");
    assert.ok(recovAck, "Stop ACK debe ser emitido para la orden recuperada");
    assert.strictEqual(recovAck.success, true);
    console.log("  ✓ Órdenes Stop perdidas durante desconexión recuperadas y procesadas con éxito.");

    sseReqRecov.destroy();
    sseSubscribers = [];
    receivedAcks = [];

    console.log("--- 8. Probando adaptador de Pi: aislamiento de sesión, abort nativo y settled ---");
    let piAbortCalled = false;
    let piEventListeners = {};

    const mockPi = {
      on: (evt, cb) => {
        piEventListeners[evt] = cb;
      },
      abort: () => {
        piAbortCalled = true;
      },
    };

    const cleanupPi = peakguardPiExtension(mockPi);

    const mockPiCtx = {
      sessionId: "pi-isolated-session",
      turnId: "turn-pi-1",
      model: "deepseek-reasoner",
      abort: () => {
        piAbortCalled = true;
      },
    };

    await piEventListeners["session_start"]?.({}, mockPiCtx);
    await piEventListeners["turn_start"]?.({}, mockPiCtx);

    await new Promise((r) => setTimeout(r, 200));

    // A. Emitir stop para sesión ajena
    console.log("  Enviando stop_requested para 'opencode-foreign-session-888'...");
    for (const sub of sseSubscribers) {
      sub.write(
        `data: ${JSON.stringify({
          type: "stop_requested",
          session_id: "opencode-foreign-session-888",
          transition_utc: "2026-10-15T01:00:00Z",
        })}\n\n`
      );
    }

    await new Promise((r) => setTimeout(r, 300));
    assert.strictEqual(piAbortCalled, false, "Pi no debe abortar si el stop_requested es para otra sesión");
    console.log("  ✓ Pi ignoró exitosamente el stop para sesión ajena.");

    // B. Emitir stop para la sesión propia de Pi
    console.log("  Enviando stop_requested para la sesión propia 'pi-isolated-session'...");
    for (const sub of sseSubscribers) {
      sub.write(
        `data: ${JSON.stringify({
          type: "stop_requested",
          command_id: "cmd_pi_001",
          client_instance_id: PI_INSTANCE_ID,
          session_id: "pi-isolated-session",
          turn_id: "turn-pi-1",
          transition_utc: "2026-10-15T01:00:00Z",
        })}\n\n`
      );
    }

    await new Promise((r) => setTimeout(r, 300));
    assert.strictEqual(piAbortCalled, true, "Pi debe abortar su sesión propia");
    const piAck = receivedAcks.find((a) => a.session_id === "pi-isolated-session");
    assert.ok(piAck, "Pi debe enviar Stop ACK");
    assert.strictEqual(piAck.success, true, "Pi debe reportar éxito de abort");
    assert.strictEqual(piAck.command_id, "cmd_pi_001");
    assert.strictEqual(piAck.client_instance_id, PI_INSTANCE_ID);
    console.log("  ✓ Pi ejecutó abort() nativo y envió Stop ACK confirmado.");

    // C. Probar agent_settled emitiendo is_active: false
    await piEventListeners["agent_settled"]?.({}, mockPiCtx);
    await new Promise((r) => setTimeout(r, 100));
    const settledHeartbeat = heartbeatCalls.find(
      (h) => h.session_id === "pi-isolated-session" && h.is_active === false
    );
    assert.ok(settledHeartbeat, "agent_settled debe enviar heartbeat con is_active: false");
    console.log("  ✓ Pi envió is_active: false en agent_settled.");

    cleanupPi();
    server.close();
    console.log("\n========================================================");
    console.log("TODAS LAS PRUEBAS DE MOCK RUNNER (T18.1) PASARON CON ÉXITO!");
    console.log("========================================================");
    process.exit(0);
  } catch (err) {
    console.error("ERROR EN PRUEBAS DE MOCK RUNNER:", err);
    server.close();
    process.exit(1);
  }
});
