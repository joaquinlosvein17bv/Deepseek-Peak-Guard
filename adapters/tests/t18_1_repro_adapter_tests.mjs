/**
 * Verificación de correcciones en adaptadores para T18.1:
 * 1. Indicador de Pi no muestra "Protegido" cuando la cobertura es PARTIAL (T18.1.1)
 * 2. Recuperación garantizada de órdenes Stop pendientes (T18.1.3)
 * 3. Identidad compuesta con client_instance_id único por instancia (T18.1.2)
 */

import http from "node:http";
import assert from "node:assert";

console.log("=== Ejecutando pruebas de verificación de adaptadores (T18.1) ===");

// 1. Probar indicador de Pi con coverage PARTIAL y PROTECTED (T18.1.1)
{
  const { formatPiStatusLabel } = await import("../pi/peakguard-pi.ts");

  const statusPartial = {
    is_peak: true,
    coverage: "PARTIAL",
    seconds_to_peak: null,
  };

  const labelPartial = formatPiStatusLabel(statusPartial);
  console.log(`[VERIF T18.1.1] Etiqueta generada por Pi cuando coverage es PARTIAL: "${labelPartial}"`);
  assert.strictEqual(labelPartial, "⛔ PEAK (PARTIAL)");
  assert.ok(
    !labelPartial.includes("Protegido"),
    "Pi NO debe mostrar 'Protegido' cuando la cobertura es PARTIAL"
  );

  const statusProtected = {
    is_peak: true,
    coverage: "PROTECTED",
    seconds_to_peak: null,
  };
  const labelProtected = formatPiStatusLabel(statusProtected);
  assert.strictEqual(labelProtected, "⛔ PEAK (Protegido)");
  console.log(`  ✓ Pi solo muestra "(Protegido)" cuando coverage === "PROTECTED".`);
}

// 2. Probar recuperación de órdenes Stop vía endpoint de pendientes (T18.1.3)
{
  let pendingOrders = [
    {
      command_id: "cmd_offline_recovery_42",
      client_instance_id: "inst_test_recovery",
      session_id: "session-offline-1",
      turn_id: "turn-off-1",
      transition_utc: "2026-10-15T01:00:00Z",
      reason: "Pico iniciado mientras el adaptador estaba offline",
    },
  ];
  let acksReceived = [];

  const mockServer = http.createServer((req, res) => {
    const url = new URL(req.url, `http://${req.headers.host}`);

    if (url.pathname === "/api/v1/adapters/commands/pending" && req.method === "GET") {
      const inst = url.searchParams.get("client_instance_id");
      const filtered = pendingOrders.filter((c) => c.client_instance_id === inst);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ client_instance_id: inst, commands: filtered }));
      return;
    }

    if (url.pathname === "/api/v1/adapters/stop-ack" && req.method === "POST") {
      let body = "";
      req.on("data", (chunk) => (body += chunk));
      req.on("end", () => {
        const parsed = JSON.parse(body);
        acksReceived.push(parsed);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "acknowledged", command_id: parsed.command_id }));
      });
      return;
    }

    if (url.pathname === "/api/v1/events") {
      res.writeHead(200, { "Content-Type": "text/event-stream" });
      res.write("data: {\"type\":\"connected\"}\n\n");
      return;
    }

    res.writeHead(404);
    res.end();
  });

  mockServer.listen(0, "127.0.0.1", async () => {
    const port = mockServer.address().port;
    process.env.PEAKGUARD_CONTROL_URL = `http://127.0.0.1:${port}`;
    process.env.PEAKGUARD_CONTROL_TOKEN = "tok-123";

    console.log("[VERIF T18.1.3] Simulando reconexión y consulta de órdenes pendientes...");

    const {
      registerActiveSession,
      startEventSubscriber,
      CLIENT_INSTANCE_ID,
    } = await import("../opencode/peakguard-opencode-server.js");

    // Reasignar la orden pendiente al CLIENT_INSTANCE_ID del proceso actual
    pendingOrders[0].client_instance_id = CLIENT_INSTANCE_ID;

    // Registrar la sesión en el mapa local
    await registerActiveSession("session-offline-1", "deepseek-chat", "deepseek_direct", "turn-off-1");

    let abortExecuted = false;
    const mockClient = {
      session: {
        abort: async ({ path }) => {
          if (path.id === "session-offline-1") {
            abortExecuted = true;
          }
          return { aborted: true };
        },
      },
    };

    const sseReq = startEventSubscriber(mockClient);
    await new Promise((r) => setTimeout(r, 400));

    assert.strictEqual(abortExecuted, true, "client.session.abort() debe ejecutarse tras recuperar la orden pendiente");
    assert.strictEqual(acksReceived.length, 1);
    assert.strictEqual(acksReceived[0].command_id, "cmd_offline_recovery_42");
    assert.strictEqual(acksReceived[0].success, true);
    console.log("  ✓ Orden de Stop pendiente recuperada y ejecutada exitosamente.");

    sseReq.destroy();
    mockServer.close();
    console.log("=== Todas las verificaciones de adaptadores pasaron exitosamente ===");
  });
}
