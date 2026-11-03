import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Persisted native and accessibility preferences.
 *
 * The document lives at `config/shortcuts.json` on the Rust side; every field
 * is optional there, so an older document keeps working after an update.
 */
export interface IntegrationSettings {
  mediaControls: boolean;
  globalShortcuts: boolean;
  notifications: boolean;
  tray: boolean;
  closeToTray: boolean;
  autostart: boolean;
  reducedMotion: boolean;
  highContrast: boolean;
}

/** Settings plus the runtime status reported by the desktop application. */
export interface IntegrationSnapshot {
  settings: IntegrationSettings;
  platform: string;
  mediaControlsActive: boolean;
  trayActive: boolean;
  shortcutsActive: boolean;
  notificationsSupported: boolean;
  autostartSupported: boolean;
  autostartEnabled: boolean;
  notices: string[];
}

export const defaultIntegrationSettings: IntegrationSettings = {
  mediaControls: true,
  globalShortcuts: true,
  notifications: true,
  tray: true,
  closeToTray: false,
  autostart: false,
  reducedMotion: false,
  highContrast: false,
};

/** Mirrors `IntegrationSettings::normalized` on the Rust side. */
export function normalizeIntegration(
  settings: IntegrationSettings,
): IntegrationSettings {
  return settings.tray ? settings : { ...settings, closeToTray: false };
}

function nativeRuntime(): boolean {
  return isTauri();
}

/** Loads the stored integration settings, or `null` in a plain browser. */
export async function loadIntegration(): Promise<IntegrationSnapshot | null> {
  if (!nativeRuntime()) return null;
  return invoke<IntegrationSnapshot>("integration_get");
}

/** Stores and applies the settings; returns the normalized snapshot. */
export async function saveIntegration(
  settings: IntegrationSettings,
): Promise<IntegrationSnapshot | null> {
  if (!nativeRuntime()) return null;
  return invoke<IntegrationSnapshot>("integration_save", {
    settings: normalizeIntegration(settings),
  });
}

/** Shows a test notification when the desktop runtime is available. */
export async function testNotification(): Promise<boolean> {
  if (!nativeRuntime()) return false;
  await invoke("integration_notification_test");
  return true;
}

/** Listens for short messages emitted by the native layer. */
export async function subscribeNativeNotices(
  handler: (message: string) => void,
): Promise<UnlistenFn> {
  if (!nativeRuntime()) return () => {};
  return listen<string>("native:notice", (event) => handler(event.payload));
}
