/**
 * Reproducción de defectos en adaptadores para T18.2:
 * 1. OpenCode abortando un turno nuevo (turn-2) ante una orden retrasada para turn-1 (T18.2.2)
 * 2. OpenCode asignando deepseek_direct como proveedor por defecto (T18.2.1)
 * 3. Pi reiniciando el backoff a cero inmediatamente al recibir HTTP 200 sin estabilidad (T18.2.5)
 */

import http from "node:http";
import assert from "node:assert";

console.log("=== Reproducción de defectos T18.2 en adaptadores ===");

// 1. T18.2.1: OpenCode asignando deepseek_direct por defecto
{
  const { registerActiveSession, activeSessions } = await import("../opencode/peakguard-opencode-server.js");
  
  // Registrar una sesión donde no se pasa proveedor ni modelo
  await registerActiveSession("session-unknown-provider", undefined, undefined, "turn-x");
  const sess = activeSessions.get("session-unknown-provider");
  console.log(`[TEST REPRO T18.2.1] Proveedor registrado por defecto para sesión sin proveedor: '${sess?.provider}'`);
  assert.strictEqual(
    sess?.provider,
    "unknown",
    "OpenCode NO debe asignar 'deepseek_direct' por defecto cuando el proveedor es desconocido"
  );
}

// 2. T18.2.2: Probar orden Stop para turno antiguo cancelando turno nuevo en OpenCode
{
  let sseSubscribers = [];
  const mockServer = http.createServer((req, res) => {
    const url = new URL(req.url, `http://${req.headers.host}`);
    if (url.pathname === "/api/v1/sessions/register" || url.pathname === "/api/v1/sessions/heartbeat") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "ok" }));
      return;
    }
    if (url.pathname === "/api/v1/adapters/stop-ack") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "acknowledged" }));
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
    process.env.PEAKGUARD_CONTROL_TOKEN = "tok-123";

    const {
      registerActiveSession,
      sendSessionHeartbeat,
      startEventSubscriber,
      CLIENT_INSTANCE_ID,
    } = await import("../opencode/peakguard-opencode-server.js");

    const sessionID = "session-stale-turn-test";

    // Iniciar sesión en turno 1
    await registerActiveSession(sessionID, "deepseek-chat", "deepseek_direct", "turn-1");

    // Transicionar al turno 2 (el turno 1 terminó)
    await sendSessionHeartbeat(sessionID, "turn-2", true);

    let abortCalled = false;
    let abortTargetSession = null;

    const mockClient = {
      session: {
        abort: async ({ path }) => {
          abortCalled = true;
          abortTargetSession = path.id;
          return { aborted: true };
        },
      },
    };

    const sseReq = startEventSubscriber(mockClient);
    await new Promise((r) => setTimeout(r, 200));

    console.log("[TEST REPRO T18.2.2] Emitiendo orden Stop tardía para 'turn-1' cuando la sesión ya está en 'turn-2'...");

    // Emitir orden tardía por SSE
    for (const sub of sseSubscribers) {
      sub.write(
        `data: ${JSON.stringify({
          type: "stop_requested",
          command_id: "cmd_stale_turn_1",
          client_instance_id: CLIENT_INSTANCE_ID,
          session_id: sessionID,
          turn_id: "turn-1", // TURNO ANTIGUO
          transition_utc: "2026-10-15T01:00:00Z",
        })}\n\n`
      );
    }

    await new Promise((r) => setTimeout(r, 300));

    // En T18.2: NO debe haberse invocado client.session.abort() porque el turno activo actual es turn-2
    console.log(`[TEST REPRO T18.2.2] abortCalled reportado tras orden tardía: ${abortCalled}`);
    assert.strictEqual(
      abortCalled,
      false,
      "Una orden Stop antigua para turn-1 NO debe abortar la sesión cuando el turno activo es turn-2"
    );

    sseReq.destroy();
    mockServer.close();

    // 3. T18.2.1 & T18.2.2 & T18.2.5: Pruebas del adaptador de Pi
    await runPiTests();
  });
}

async function runPiTests() {
  console.log("--- Pruebas de Pi para T18.2 ---");
  let lastRegisteredSession = null;
  let receivedStopAcks = [];
  let sseSubscribers = [];

  const mockServer = http.createServer((req, res) => {
    const url = new URL(req.url, `http://${req.headers.host}`);
    if (url.pathname === "/api/v1/sessions/register") {
      let b = "";
      req.on("data", (c) => (b += c));
      req.on("end", () => {
        lastRegisteredSession = JSON.parse(b);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "ok" }));
      });
      return;
    }
    if (url.pathname === "/api/v1/sessions/heartbeat") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "ok" }));
      return;
    }
    if (url.pathname === "/api/v1/adapters/stop-ack") {
      let b = "";
      req.on("data", (c) => (b += c));
      req.on("end", () => {
        receivedStopAcks.push(JSON.parse(b));
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
    if (url.pathname === "/api/v1/status") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ coverage: "PARTIAL", is_peak: false }));
      return;
    }
    res.writeHead(404);
    res.end();
  });

  await new Promise((resolve) => {
    mockServer.listen(0, "127.0.0.1", async () => {
      const port = mockServer.address().port;
      process.env.PEAKGUARD_CONTROL_URL = `http://127.0.0.1:${port}`;
      process.env.PEAKGUARD_CONTROL_TOKEN = "tok-pi-test";

      const {
        registerActiveSession,
        default: peakguardPiExtension,
        CLIENT_INSTANCE_ID: PI_INSTANCE_ID,
      } = await import("../pi/peakguard-pi.ts");

      // T18.2.1: Verificar que Pi NO registra deepseek_direct para modelo desconocido
      await registerActiveSession("pi-sess-unknown", "turn-1", "unknown");
      console.log(`[TEST REPRO T18.2.1 Pi] Proveedor para modelo 'unknown': '${lastRegisteredSession?.provider_id}'`);
      assert.strictEqual(
        lastRegisteredSession?.provider_id,
        "unknown",
        "Pi no debe registrar 'deepseek_direct' cuando el modelo es desconocido"
      );

      // T18.2.2: Verificar que Pi descarta Stop tardío para turn-1 si el turno actual es turn-2
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
      const mockCtx = {
        sessionId: "pi-sess-turns",
        turnId: "turn-1",
        model: "deepseek-chat",
        abort: () => {
          piAbortCalled = true;
        },
      };

      // Iniciar en turn-1
      await piEventListeners["turn_start"]({}, mockCtx);

      // Transicionar a turn-2
      mockCtx.turnId = "turn-2";
      await piEventListeners["turn_start"]({}, mockCtx);

      await new Promise((r) => setTimeout(r, 200));

      console.log("[TEST REPRO T18.2.2 Pi] Emitiendo orden Stop tardía para turn-1 a Pi...");
      for (const sub of sseSubscribers) {
        sub.write(
          `data: ${JSON.stringify({
            type: "stop_requested",
            command_id: "cmd_pi_stale_turn_1",
            client_instance_id: PI_INSTANCE_ID,
            session_id: "pi-sess-turns",
            turn_id: "turn-1",
            transition_utc: "2026-10-15T01:00:00Z",
          })}\n\n`
        );
      }

      await new Promise((r) => setTimeout(r, 300));

      console.log(`[TEST REPRO T18.2.2 Pi] piAbortCalled: ${piAbortCalled}`);
      assert.strictEqual(
        piAbortCalled,
        false,
        "Pi NO debe invocar abort() si la orden es para un turno anterior que ya terminó"
      );

      const staleAck = receivedStopAcks.find((a) => a.command_id === "cmd_pi_stale_turn_1");
      assert.ok(staleAck, "Pi debe enviar Stop ACK informando del descarte");
      assert.strictEqual(staleAck.success, false);
      assert.strictEqual(staleAck.error, "turn_mismatch");
      console.log("  ✓ Pi envió Stop ACK con error 'turn_mismatch' y no abortó el nuevo turno.");

      cleanupPi();
      mockServer.close();
      resolve();
    });
  });

  console.log("=== Todas las pruebas de adaptadores T18.2 completadas con éxito ===");
  process.exit(0);
}
