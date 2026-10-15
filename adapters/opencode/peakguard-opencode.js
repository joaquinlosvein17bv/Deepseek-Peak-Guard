// @ts-nocheck
/**
 * PeakGuard Adapter Unificado para OpenCode (v1.18.35+)
 * Módulo de compatibilidad que reexporta el Server Plugin y el TUI Widget.
 */

import serverDefault, {
  PeakGuardServerPlugin,
  getAuthToken,
  sendControlRequest,
  registerActiveSession,
  sendSessionHeartbeat,
  unregisterSession,
  sendStopAck,
  startEventSubscriber,
} from "./peakguard-opencode-server.js";

import tuiDefault, { tui as PeakGuardTuiPlugin, fetchStatus } from "./peakguard-tui-sidebar.js";

export {
  PeakGuardServerPlugin,
  PeakGuardTuiPlugin,
  getAuthToken,
  sendControlRequest,
  registerActiveSession,
  sendSessionHeartbeat,
  unregisterSession,
  sendStopAck,
  startEventSubscriber,
  fetchStatus,
};

export const PeakGuardSetupPlugin = (api) => {
  startEventSubscriber(api?.client);
};

export default {
  id: "peakguard.opencode",
  server: PeakGuardServerPlugin,
  setup: PeakGuardSetupPlugin,
  tui: PeakGuardTuiPlugin,
};
