/**
 * Reproducción de Fallo F: El adaptador Pi aborta un turno activo
 * incluso cuando la orden de Stop está dirigida a OTRA sesión distinta.
 */

import http from "node:http";
import assert from "node:assert";

const TEST_TOKEN = "test-token-repro";
let sseSubscribers = [];

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  if (url.pathname === "/api/v1/status") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ mode: "restricted_stop", is_peak: true, coverage: "PROTECTED" }));
    return;
  }
  if (url.pathname === "/api/v1/events") {
    res.writeHead(200, { "Content-Type": "text/event-stream" });
    res.write("data: {\"type\":\"connected\"}\n\n");
    sseSubscribers.push(res);
    return;
  }
  if (url.pathname === "/api/v1/adapters/stop-ack") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "acknowledged" }));
    return;
  }
  res.writeHead(404);
  res.end();
});

server.listen(0, "127.0.0.1", async () => {
  const port = server.address().port;
  process.env.PEAKGUARD_CONTROL_URL = `http://127.0.0.1:${port}`;
  process.env.PEAKGUARD_CONTROL_TOKEN = TEST_TOKEN;

  // Importar adaptador de Pi
  const { default: peakguardExtension } = await import("../pi/peakguard-pi.ts");

  let abortCalled = false;
  let eventListeners = {};

  const mockPi = {
    on: (evt, cb) => {
      eventListeners[evt] = cb;
    },
    abort: () => {
      abortCalled = true;
    },
  };

  const cleanup = peakguardExtension(mockPi);

  // Iniciar sesión y turno en Pi para la sesión 'pi-session-123'
  const mockCtx = {
    sessionId: "pi-session-123",
    turnId: "turn-abc",
    model: "deepseek-chat",
    abort: () => {
      abortCalled = true;
    },
  };

  await eventListeners["session_start"]?.({}, mockCtx);
  await eventListeners["turn_start"]?.({}, mockCtx);

  // Esperar a que la conexión SSE esté abierta
  await new Promise((r) => setTimeout(r, 200));

  // EMITIR UN STOP DIRIGIDO A UNA SESIÓN AJENA 'opencode-foreign-session-999'
  console.log("[REPRO F] Emitiendo orden de Stop para sesión ajena 'opencode-foreign-session-999'...");
  for (const sub of sseSubscribers) {
    sub.write(
      `data: ${JSON.stringify({
        type: "stop_requested",
        session_id: "opencode-foreign-session-999",
        turn_id: "turn-xyz",
        transition_utc: "2026-10-15T01:00:00Z",
      })}\n\n`
    );
  }

  await new Promise((r) => setTimeout(r, 300));

  console.log(`[REPRO F] ¿activeCtx.abort() fue llamado?: ${abortCalled}`);
  
  cleanup();
  server.close();

  // REPRODUCCIÓN DEL FALLO:
  // En el código actual, abortCalled es TRUE porque no valida session_id!
  assert.strictEqual(
    abortCalled,
    false,
    "Confirma que Pi NO aborta su turno activo cuando la orden es para otra sesión"
  );
  console.log("✓ Corrección F verificada: Pi ignora órdenes de Stop de sesiones ajenas.");
  process.exit(0);
});
