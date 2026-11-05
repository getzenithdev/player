import { check, type Update } from "@tauri-apps/plugin-updater";
import { isTauri } from "@tauri-apps/api/core";

export type UpdateChannel = "stable" | "beta";

const CHANNEL_KEY = "zenith-player.update-channel";
const SEMVER = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?/;

export interface AvailableUpdate {
  update: Update;
  channel: UpdateChannel;
}

export function getUpdateChannel(): UpdateChannel {
  if (typeof localStorage === "undefined") return "stable";
  return localStorage.getItem(CHANNEL_KEY) === "beta" ? "beta" : "stable";
}

export function saveUpdateChannel(channel: UpdateChannel): void {
  localStorage.setItem(CHANNEL_KEY, channel);
}

function compareVersions(left: string, right: string): number {
  const a = SEMVER.exec(left);
  const b = SEMVER.exec(right);
  if (!a || !b) return 0;
  for (let index = 1; index <= 3; index += 1) {
    const difference = Number(a[index]) - Number(b[index]);
    if (difference) return difference;
  }
  if (!a[4] && b[4]) return 1;
  if (a[4] && !b[4]) return -1;
  return (a[4] ?? "").localeCompare(b[4] ?? "");
}

/**
 * The native updater verifies the minisign signature before returning an
 * update. The channel claim is checked here so a release from another feed
 * can never be installed accidentally.
 */
export async function checkForUpdate(
  channel = getUpdateChannel(),
): Promise<AvailableUpdate | null> {
  if (!isTauri()) return null;
  const update = await check();
  if (!update || compareVersions(update.version, update.currentVersion) <= 0) {
    await update?.close();
    return null;
  }
  const manifestChannel = update.rawJson.channel;
  if (manifestChannel !== undefined && manifestChannel !== channel) {
    await update.close();
    return null;
  }
  return { update, channel };
}

export async function installUpdate(
  available: AvailableUpdate,
  onProgress: (progress: number | null) => void,
): Promise<void> {
  let total = 0;
  let downloaded = 0;
  await available.update.download((event) => {
    if (event.event === "Started") total = event.data.contentLength ?? 0;
    if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      onProgress(total ? Math.round((downloaded / total) * 100) : null);
    }
    if (event.event === "Finished") onProgress(100);
  });
  await available.update.install({ restartAfterInstall: true });
}
