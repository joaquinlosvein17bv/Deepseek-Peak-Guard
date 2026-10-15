// @ts-nocheck
/**
 * PeakGuard Sidebar & Statusline Widget para OpenCode TUI
 * Diseñado conforme a la especificación funcional §5.2 de PeakGuard.
 * Compatible con OpenCode v1.18.35+ (tui.jsonc)
 */

import http from "node:http";

const PEAKGUARD_CONTROL_URL = process.env.PEAKGUARD_CONTROL_URL || "http://127.0.0.1:8766";
const id = "peakguard-sidebar";

// Estado inicial estricto: NUNCA asumir PROTEGIDO sin validación real
let currentState = {
  is_peak: false,
  mode: "unknown",
  next_peak_utc: null,
  seconds_to_peak: null,
  coverage: "DISCONNECTED",
};

let countdownInterval = null;

// Sincronización periódica liviana con el demonio local
export async function fetchStatus() {
  return new Promise((resolve) => {
    http
      .get(`${PEAKGUARD_CONTROL_URL}/api/v1/status`, (res) => {
        let raw = "";
        res.on("data", (chunk) => (raw += chunk));
        res.on("end", () => {
          try {
            if (res.statusCode === 200) {
              currentState = JSON.parse(raw);
              resolve(currentState);
            } else {
              currentState.coverage = "DISCONNECTED";
              resolve(null);
            }
          } catch {
            currentState.coverage = "DISCONNECTED";
            resolve(null);
          }
        });
      })
      .on("error", () => {
        currentState.coverage = "DISCONNECTED";
        resolve(null);
      });
  });
}

function formatCountdown(totalSeconds) {
  if (totalSeconds == null || totalSeconds < 0) return "--:--:--";
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = totalSeconds % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export const tui = async (api) => {
  // Consulta inicial y actualización periódica cada 10s (recalculando cuenta atrás localmente cada segundo)
  await fetchStatus();
  setInterval(fetchStatus, 10000);

  setInterval(() => {
    if (currentState.seconds_to_peak != null && currentState.seconds_to_peak > 0) {
      currentState.seconds_to_peak -= 1;
    }
  }, 1000);

  // Registro en slots de sidebar y barra de estado si están disponibles en la versión instalada
  if (api?.slots?.register) {
    api.slots.register({
      id,
      order: 50,
      slots: {
        sidebar_content() {
          const isDisconnected = currentState.coverage === "DISCONNECTED";
          const isPeak = currentState.is_peak;
          const statusDot = isDisconnected
            ? "○ DESCONECTADO"
            : isPeak
            ? "● PEAK ACTIVO"
            : "● OFF-PEAK";
          const countdown = formatCountdown(currentState.seconds_to_peak);
          const isFinalCountdown =
            !isDisconnected &&
            currentState.seconds_to_peak != null &&
            currentState.seconds_to_peak <= 10 &&
            currentState.seconds_to_peak > 0;
          const modeStr = (currentState.mode || "unknown").toUpperCase();
          const coverageStr = currentState.coverage || "DISCONNECTED";

          return {
            title: `PeakGuard [${statusDot}]`,
            lines: [
              `Modo: ${isDisconnected ? "DESCONECTADO" : modeStr}`,
              `Cobertura: ${coverageStr}`,
              `Próximo peak: ${currentState.next_peak_utc ? currentState.next_peak_utc.slice(11, 16) + " UTC" : (isPeak ? "En curso" : "--:-- UTC")}`,
              isFinalCountdown
                ? `⚠️ CUENTA ATRÁS: ${currentState.seconds_to_peak}s`
                : `Cuenta atrás: ${isDisconnected ? "--:--:--" : countdown}`,
              `Alertas: 30 · 10 · 5 min`,
            ],
          };
        },
        status_bar() {
          if (currentState.coverage === "DISCONNECTED") {
            return "⚠️ PeakGuard: DESCONECTADO";
          }
          const isPeak = currentState.is_peak;
          const label = isPeak
            ? "⛔ PeakGuard: PEAK"
            : `🛡️ PeakGuard: OFF-PEAK (${formatCountdown(currentState.seconds_to_peak)})`;
          return label;
        },
      },
    });
  }
};

export default {
  id: "peakguard-sidebar",
  tui,
  setup() {},
};
