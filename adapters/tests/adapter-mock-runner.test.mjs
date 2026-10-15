/**
 * PeakGuard Adapter Mock Verification Suite
 * Valida el comportamiento de los adaptadores de OpenCode y Pi en Node.js
 * contra un servidor Control API simulado.
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
    } = await import("../opencode/peakguard-opencode-server.js");

    const { fetchStatus } = await import("../opencode/peakguard-tui-sidebar.js");
    const { default: peakguardPiExtension } = await import("../pi/peakguard-pi.ts");

    console.log("--- 1. Probando fetchStatus() en Widget TUI ---");
    const status = await fetchStatus();
    assert.strictEqual(status.coverage, "PROTECTED");
    assert.strictEqual(status.mode, "restricted_stop");
    console.log("  ✓ Widget TUI sincronizó estado correctamente.");

    console.log("--- 2. Probando registro de sesiones activas (T17.2, T18.4) ---");
    const regRes = await registerActiveSession("session-test-1", "deepseek-chat", "deepseek_direct", "turn-1");
    assert.strictEqual(regRes.statusCode, 200);
    assert.strictEqual(registeredSessions.length, 1);
    assert.strictEqual(registeredSessions[0].session_id, "session-test-1");
    console.log("  ✓ Sesión registrada con token Bearer verificado.");

    console.log("--- 3. Probando Stop nativo exitoso en OpenCode (T17.2, T17.6) ---");
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
    assert.strictEqual(receivedAcks[0].error, null);
    console.log("  ✓ client.session.abort() ejecutado y confirmado con Stop ACK exitoso.");

    sseReq.destroy();
    sseSubscribers = [];
    receivedAcks = [];

    console.log("--- 4. Probando Stop con fallo: NUNCA reportar falso positivo (T17.1, T17.6) ---");
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

    console.log("--- 5. Probando aislamiento de sesión en OpenCode (ignora sesiones ajenas) ---");
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

    console.log("--- 6. Probando adaptador de Pi: aislamiento de sesión, abort nativo y settled ---");
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

    // B. Emitir stop para la sesión activa de Pi
    console.log("  Enviando stop_requested para la sesión propia 'pi-isolated-session'...");
    for (const sub of sseSubscribers) {
      sub.write(
        `data: ${JSON.stringify({
          type: "stop_requested",
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
    console.log("\n=========================================");
    console.log("TODAS LAS PRUEBAS DE MOCK RUNNER PASARON!");
    console.log("=========================================");
    process.exit(0);
  } catch (err) {
    console.error("ERROR EN PRUEBAS DE MOCK RUNNER:", err);
    server.close();
    process.exit(1);
  }
});
