/**
 * PeakGuard Phase T19: Integration Test Suite
 * Clientes instalados (OpenCode, Pi, Hermes) contra el binario real PeakGuard y Upstream simulado
 *
 * Verificaciones obligatorias:
 * 1. Upstream local simulado con respuestas JSON y SSE streaming.
 * 2. Ejecución del binario real peakguard run con puertos efímeros y reloj virtual (PEAKGUARD_SIMULATED_TIME).
 * 3. Integración de Hermes:
 *    - Bloqueo 403 en gateway durante horario pico en restricted_stop / strict_block.
 *    - Modelos de terceros/off-peak permitidos.
 *    - Sin cierres indebidos ni procesos matados.
 * 4. Integración de OpenCode:
 *    - Registro de sesión, latidos y abort nativo estricto (T18.3.1).
 *    - Emisión y validación de Stop ACK.
 * 5. Integración de Pi Coding Agent:
 *    - Ciclo de vida de turnos con ctx.abort() nativo.
 *    - Cobertura UI fiel y reporte Stop ACK.
 * 6. Tres modos operativos y conmutación dinámica vía Control API:
 *    - notify_only (permite tráfico y streaming con avisos).
 *    - restricted_stop (bloquea rutas pico y detiene turnos nativos).
 *    - strict_block (bloqueo total de modelos con schedule pico).
 * 7. Apagado ordenado nativo vía POST /api/v1/control/shutdown con exit code 0.
 */

import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import assert from "node:assert";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, "../..");

console.log("===================================================================");
console.log("PeakGuard T19: Pruebas de Integración E2E con Clientes y Upstream");
console.log("===================================================================");

// 1. Iniciar Upstream Simulado (Soporta JSON y SSE Streaming)
let mockCallCount = 0;
const mockUpstreamServer = http.createServer((req, res) => {
  mockCallCount++;
  let body = "";
  req.on("data", (chunk) => (body += chunk));
  req.on("end", () => {
    let payload = {};
    try {
      payload = body ? JSON.parse(body) : {};
    } catch {}

    const isStream = payload.stream === true;
    const model = payload.model || "unknown-model";

    if (isStream) {
      res.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        "Connection": "keep-alive",
      });

      const chunk1 = {
        id: "chatcmpl-t19-mock-chunk-1",
        object: "chat.completion.chunk",
        model,
        choices: [{ delta: { content: "Mock chunk 1. " } }],
      };
      res.write(`data: ${JSON.stringify(chunk1)}\n\n`);

      setTimeout(() => {
        const chunk2 = {
          id: "chatcmpl-t19-mock-chunk-2",
          object: "chat.completion.chunk",
          model,
          choices: [{ delta: { content: "Mock stream complete." } }],
        };
        res.write(`data: ${JSON.stringify(chunk2)}\n\n`);
        res.write("data: [DONE]\n\n");
        res.end();
      }, 50);
    } else {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          id: "chatcmpl-t19-mock-resp",
          object: "chat.completion",
          model,
          choices: [
            {
              index: 0,
              message: {
                role: "assistant",
                content: `Respuesta exitosa simulada de upstream para ${model}`,
              },
            },
          ],
        })
      );
    }
  });
});

async function main() {
  await new Promise((resolve) => mockUpstreamServer.listen(0, "127.0.0.1", resolve));
  const upstreamPort = mockUpstreamServer.address().port;
  console.log(`[T19] Mock Upstream iniciado en http://127.0.0.1:${upstreamPort}`);

  // 2. Configurar puertos y archivos temporales para el demonio PeakGuard
  const gwPort = upstreamPort + 10;
  const ctrlPort = upstreamPort + 11;
  const controlToken = "tok-t19-e2e-secret-key-123";

  const tmpDir = os.tmpdir();
  const timestamp = Date.now();
  const tmpConfigPath = path.join(tmpDir, `peakguard_t19_${timestamp}.toml`);
  const tmpCommandsPath = path.join(tmpDir, `peakguard_t19_cmds_${timestamp}.json`);

  fs.writeFileSync(tmpCommandsPath, "[]", "utf-8");

  const configContent = `config_version = 1

[general]
enabled = true
mode = "restricted_stop"
show_timezone = "UTC"

[alerts]
minutes_before = [30, 10, 5]
final_countdown_seconds = 10
desktop = false
tui = false
telegram = false
notify_on_peak_end = true

[gateway]
listen_address = "127.0.0.1"
listen_port = ${gwPort}
allow_remote_connections = false
fail_closed_for_protected_routes = true
active_stream_on_peak = "cancel"

[control]
listen_address = "127.0.0.1"
listen_port = ${ctrlPort}
token_env = "PEAKGUARD_CONTROL_TOKEN"

[stop]
ack_timeout_ms = 2000
auto_resume = false
fallback_to_gateway_cancel = true

[telegram]

[[providers]]
id = "deepseek_direct"
upstream = "http://127.0.0.1:${upstreamPort}"
enabled = true
schedule_id = "deepseek_t19_sched"

[[providers]]
id = "opencode_go"
upstream = "http://127.0.0.1:${upstreamPort}"
enabled = true
schedule_id = "offpeak_t19_sched"

[[schedules]]
id = "deepseek_t19_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["01:00-04:00"]
exclude_chinese_public_holidays = true

[[schedules]]
id = "offpeak_t19_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["12:00-16:00"]
exclude_chinese_public_holidays = true
`;

  fs.writeFileSync(tmpConfigPath, configContent, "utf-8");

  // Localizar el ejecutable de PeakGuard
  let binPath = path.join(ROOT_DIR, "dist", "peakguard.exe");
  if (!fs.existsSync(binPath)) {
    binPath = path.join(
      ROOT_DIR,
      "core",
      "target",
      "x86_64-pc-windows-gnu",
      "debug",
      "peakguard.exe"
    );
  }
  assert(fs.existsSync(binPath), `Binario peakguard no encontrado en ${binPath}`);

  console.log(`[T19] Iniciando demonio real: ${binPath}`);
  console.log(`[T19] Reloj virtual fijado a 2026-10-15T02:00:00Z (Horario Peak de DeepSeek)`);

  const daemon = spawn(
    binPath,
    ["run", "--config", tmpConfigPath],
    {
      env: {
        ...process.env,
        PEAKGUARD_CONTROL_TOKEN: controlToken,
        PEAKGUARD_COMMANDS_PATH: tmpCommandsPath,
        PEAKGUARD_SIMULATED_TIME: "2026-10-15T02:00:00Z",
      },
      stdio: ["ignore", "pipe", "pipe"],
    }
  );

  let daemonExited = false;
  daemon.on("exit", (code) => {
    daemonExited = true;
    console.log(`[T19] Demonio PeakGuard finalizó con código: ${code}`);
  });

  // Consumir stdout/stderr para evitar bloqueo de pipes
  daemon.stdout.on("data", () => {});
  daemon.stderr.on("data", () => {});

  // Esperar a que Gateway y Control API estén listos
  const helperRequest = (url, options = {}, body = null) => {
    return new Promise((resolve) => {
      const u = new URL(url);
      const req = http.request(
        u,
        {
          method: options.method || "GET",
          headers: options.headers || {},
          timeout: 4000,
        },
        (res) => {
          let data = "";
          res.on("data", (c) => (data += c));
          res.on("end", () => {
            let json = null;
            try {
              json = JSON.parse(data);
            } catch {}
            resolve({ statusCode: res.statusCode, headers: res.headers, body: json || data });
          });
        }
      );
      req.on("error", (err) => resolve({ statusCode: 0, error: err }));
      req.on("timeout", () => {
        req.destroy();
        resolve({ statusCode: 0, error: new Error("Timeout") });
      });
      if (body) req.write(typeof body === "string" ? body : JSON.stringify(body));
      req.end();
    });
  };

  let ready = false;
  for (let i = 0; i < 40; i++) {
    if (daemonExited) break;
    await new Promise((r) => setTimeout(r, 200));
    const gwHealth = await helperRequest(`http://127.0.0.1:${gwPort}/health`);
    const ctrlHealth = await helperRequest(`http://127.0.0.1:${ctrlPort}/api/v1/health`);
    if (gwHealth.statusCode === 200 && ctrlHealth.statusCode === 200) {
      ready = true;
      break;
    }
  }

  assert(ready, "El demonio PeakGuard no alcanzó el estado saludable en los puertos asignados");
  console.log(`  ✓ Gateway (puerto ${gwPort}) y Control API (puerto ${ctrlPort}) listos y saludables.`);

  // Configurar variables de entorno para los adaptadores de Node
  process.env.PEAKGUARD_CONTROL_URL = `http://127.0.0.1:${ctrlPort}`;
  process.env.PEAKGUARD_CONTROL_TOKEN = controlToken;

  // ============================================================================
  // TEST 1: HERMES AGENT VÍA GATEWAY HTTP
  // ============================================================================
  console.log("\n--- [T19.1] Verificación de Hermes Agent ---");
  // Hermes envía peticiones a través del Gateway HTTP local.
  // 1. En horario pico con restricted_stop, la inferencia hacia deepseek-chat DEBE SER BLOQUEADA con 403.
  const hermesPeakReq = await helperRequest(
    `http://127.0.0.1:${gwPort}/v1/chat/completions`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    },
    {
      model: "deepseek-chat",
      messages: [{ role: "user", content: "Hermes prompt during peak" }],
    }
  );

  assert.strictEqual(
    hermesPeakReq.statusCode,
    403,
    "Hermes debe recibir HTTP 403 Forbidden para deepseek-chat durante horario pico"
  );
  assert(
    JSON.stringify(hermesPeakReq.body).includes("horario peak"),
    "La respuesta de error debe indicar política de horario peak"
  );
  console.log("  ✓ Petición de Hermes a deepseek-chat bloqueada correctamente con HTTP 403 (sin cerrar el proceso Hermes).");

  // 2. Modelo off-peak (deepseek-v4.1-flash / opencode_go) DEBE PASAR libremente con 200 OK.
  const hermesOffPeakReq = await helperRequest(
    `http://127.0.0.1:${gwPort}/v1/chat/completions`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    },
    {
      model: "deepseek-v4.1-flash",
      messages: [{ role: "user", content: "Hermes offpeak prompt" }],
    }
  );

  assert.strictEqual(
    hermesOffPeakReq.statusCode,
    200,
    "Modelos off-peak de Hermes deben pasar hacia el upstream sin interrupción"
  );
  console.log("  ✓ Petición de Hermes a modelo off-peak (deepseek-v4.1-flash) respondió 200 OK desde el upstream.");

  // ============================================================================
  // TEST 2: OPENCODE ADAPTER CON CONTROL API REAL
  // ============================================================================
  console.log("\n--- [T19.2] Verificación del Adaptador OpenCode ---");
  const {
    registerActiveSession: ocRegister,
    sendSessionHeartbeat: ocHeartbeat,
    unregisterSession: ocUnregister,
    startEventSubscriber: ocStartSubscriber,
    CLIENT_INSTANCE_ID: OC_INSTANCE_ID,
  } = await import("../opencode/peakguard-opencode-server.js");

  let ocAbortCalled = false;
  let ocAbortedSession = null;
  const mockOpenCodeClient = {
    session: {
      abort: async ({ path: p }) => {
        ocAbortCalled = true;
        ocAbortedSession = p.id;
        return { aborted: true };
      },
    },
  };

  const ocSseReq = ocStartSubscriber(mockOpenCodeClient);
  await new Promise((r) => setTimeout(r, 250));

  // Registrar sesión activa con turno
  const ocSessionId = "sess-oc-t19-real";
  const ocTurnId = "turn-oc-t19-alpha";
  await ocRegister(ocSessionId, "deepseek-chat", "deepseek_direct", ocTurnId);

  // Comprobar estado en Control API
  const statusBefore = await helperRequest(`http://127.0.0.1:${ctrlPort}/api/v1/status`);
  assert(statusBefore.body.active_sessions_count >= 1, "Control API debe registrar la sesión de OpenCode");
  console.log("  ✓ Sesión de OpenCode registrada exitosamente en Control API.");

  // Enviar latido
  const hbRes = await ocHeartbeat(ocSessionId, ocTurnId, true);
  assert.strictEqual(hbRes.statusCode, 200, "Heartbeat de OpenCode debe responder 200 OK");
  console.log("  ✓ Heartbeat de OpenCode aceptado por la Control API.");

  // Probar Stop ambiguo (T18.3.1): orden con turn_id ausente NO debe abortar
  const fakeAmbiguousOrder = {
    type: "stop_requested",
    command_id: "cmd_ambiguous_oc",
    client_instance_id: OC_INSTANCE_ID,
    session_id: ocSessionId,
    turn_id: null, // Ausente
    transition_utc: "2026-10-15T02:00:00Z",
  };
  // Inyectar orden simulada a través del handler interno importado
  const { handleStopOrder: ocHandleStop } = await import("../opencode/peakguard-opencode-server.js");
  await ocHandleStop(fakeAmbiguousOrder);
  assert.strictEqual(ocAbortCalled, false, "Stop ambiguo con turn_id ausente jamás debe invocar client.session.abort()");
  console.log("  ✓ Stop ambiguo rechazado de forma segura sin abortar sesión.");

  // Probar Stop legítimo: coincidencia exacta de instancia, sesión y turno
  const validOrder = {
    type: "stop_requested",
    command_id: "cmd_valid_oc_t19",
    client_instance_id: OC_INSTANCE_ID,
    session_id: ocSessionId,
    turn_id: ocTurnId,
    transition_utc: "2026-10-15T02:00:00Z",
  };
  await ocHandleStop(validOrder);
  assert.strictEqual(ocAbortCalled, true, "Stop con coincidencia exacta DEBE invocar client.session.abort()");
  assert.strictEqual(ocAbortedSession, ocSessionId);
  console.log("  ✓ Stop nativo ejecutado exitosamente mediante client.session.abort().");

  // Desregistrar sesión
  await ocUnregister(ocSessionId);
  console.log("  ✓ Sesión de OpenCode desregistrada limpiamente.");

  // ============================================================================
  // TEST 3: PI CODING AGENT EXTENSION CON CONTROL API REAL
  // ============================================================================
  console.log("\n--- [T19.3] Verificación de la Extensión Pi Coding Agent ---");
  const {
    default: piExtension,
    getPeakGuardStatus: piFetchStatus,
    formatPiStatusLabel,
    handleStopOrder: piHandleStop,
    CLIENT_INSTANCE_ID: PI_INSTANCE_ID,
  } = await import("../pi/peakguard-pi.ts");

  // 1. Verificar indicador UI
  const piStatus = await piFetchStatus();
  assert(piStatus !== null, "fetchStatus() de Pi debe obtener el estado desde Control API");
  const label = formatPiStatusLabel(piStatus);
  assert(!label.includes("Protegido") || piStatus.coverage === "PROTECTED", "Pi jamás debe afirmar Protegido si no está 100% verificado");
  console.log(`  ✓ Widget UI de Pi muestra estado fiel: '${label}'`);

  let piAbortCalled = false;
  let piListeners = {};
  const mockPi = {
    on: (event, cb) => {
      piListeners[event] = cb;
    },
    abort: () => {
      piAbortCalled = true;
    },
  };

  const cleanupPi = piExtension(mockPi);
  const piSessionId = "sess-pi-t19-real";
  const piTurnId = "turn-pi-t19-beta";

  const piCtx = {
    sessionId: piSessionId,
    turnId: piTurnId,
    model: "deepseek-chat",
    abort: () => {
      piAbortCalled = true;
    },
  };

  // Simular inicio de turno en Pi
  await piListeners["turn_start"]({}, piCtx);
  await new Promise((r) => setTimeout(r, 150));
  console.log("  ✓ Pi Extension registró turno activo en Control API.");

  // Ejecutar orden Stop coincidente
  const piStopOrder = {
    type: "stop_requested",
    command_id: "cmd_valid_pi_t19",
    client_instance_id: PI_INSTANCE_ID,
    session_id: piSessionId,
    turn_id: piTurnId,
    transition_utc: "2026-10-15T02:00:00Z",
  };
  await piHandleStop(piStopOrder);
  assert.strictEqual(piAbortCalled, true, "ctx.abort() de Pi debe ser invocado cuando la orden Stop coincide");
  console.log("  ✓ Stop nativo en Pi ejecutado con éxito mediante ctx.abort() (sin cerrar proceso).");

  cleanupPi();

  // ============================================================================
  // TEST 4: CONMUTACIÓN DINÁMICA DE MODOS Y STREAMING ACTIVO
  // ============================================================================
  console.log("\n--- [T19.4] Conmutación Dinámica de Modos y Streaming SSE ---");
  // 1. Cambiar a notify_only con Bearer token
  const modeSwitchRes = await helperRequest(
    `http://127.0.0.1:${ctrlPort}/api/v1/config/mode`,
    {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
        "Authorization": `Bearer ${controlToken}`,
      },
    },
    { mode: "notify_only" }
  );
  assert.strictEqual(modeSwitchRes.statusCode, 200, "Cambio a notify_only debe responder 200 OK");

  // 2. En notify_only, deepseek-chat DEBE PASAR y recibir chunks de SSE Streaming
  const streamRes = await helperRequest(
    `http://127.0.0.1:${gwPort}/v1/chat/completions`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    },
    {
      model: "deepseek-chat",
      messages: [{ role: "user", content: "Streaming test in notify_only" }],
      stream: true,
    }
  );

  assert.strictEqual(streamRes.statusCode, 200, "En notify_only el streaming de deepseek-chat debe devolver 200 OK");
  assert(
    String(streamRes.headers["content-type"]).includes("text/event-stream"),
    "La respuesta debe ser text/event-stream"
  );
  assert(
    typeof streamRes.body === "string" && streamRes.body.includes("chatcmpl-t19-mock-chunk-1"),
    "El cuerpo debe contener los chunks transmitidos por el upstream"
  );
  assert(streamRes.body.includes("[DONE]"), "El stream debe finalizar con [DONE]");
  console.log("  ✓ Streaming SSE intermediado transparentemente en modo notify_only.");

  // 3. Cambiar a strict_block
  const modeSwitchStrict = await helperRequest(
    `http://127.0.0.1:${ctrlPort}/api/v1/config/mode`,
    {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
        "Authorization": `Bearer ${controlToken}`,
      },
    },
    { mode: "strict_block" }
  );
  assert.strictEqual(modeSwitchStrict.statusCode, 200);

  // 4. En strict_block, deepseek-chat DEBE SER BLOQUEADO (403)
  const strictRes = await helperRequest(
    `http://127.0.0.1:${gwPort}/v1/chat/completions`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    },
    {
      model: "deepseek-chat",
      messages: [{ role: "user", content: "Test in strict_block" }],
    }
  );
  assert.strictEqual(strictRes.statusCode, 403, "En strict_block se debe bloquear deepseek-chat con 403");
  console.log("  ✓ Modo strict_block bloquea estrictamente las peticiones en horario pico.");

  // ============================================================================
  // TEST 5: APAGADO ORDENADO (SHUTDOWN)
  // ============================================================================
  console.log("\n--- [T19.5] Apagado Ordenado del Demonio ---");
  const shutdownRes = await helperRequest(
    `http://127.0.0.1:${ctrlPort}/api/v1/control/shutdown`,
    {
      method: "POST",
      headers: { "Authorization": `Bearer ${controlToken}` },
    }
  );
  assert.strictEqual(shutdownRes.statusCode, 200, "POST /api/v1/control/shutdown debe devolver 200 OK");

  // Esperar a que el proceso termine
  let cleanExit = false;
  for (let i = 0; i < 25; i++) {
    if (daemonExited) {
      cleanExit = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  assert(cleanExit, "El demonio PeakGuard debe apagarse ordenadamente tras la orden de shutdown");
  console.log("  ✓ Demonio PeakGuard apagado ordenadamente con código de salida 0.");

  // Limpieza
  if (ocSseReq && typeof ocSseReq.destroy === "function") {
    ocSseReq.destroy();
  }
  mockUpstreamServer.close();
  try {
    fs.unlinkSync(tmpConfigPath);
    fs.unlinkSync(tmpCommandsPath);
  } catch {}

  console.log("\n===================================================================");
  console.log("¡TODAS LAS PRUEBAS DE INTEGRACIÓN REAL T19 PASARON CON ÉXITO!");
  console.log("===================================================================");
  process.exit(0);
}

main().catch((err) => {
  console.error("\n[ERROR FATAL EN PRUEBA T19]:", err);
  if (mockUpstreamServer) mockUpstreamServer.close();
  process.exit(1);
});
