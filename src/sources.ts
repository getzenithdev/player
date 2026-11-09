import { invoke } from "@tauri-apps/api/core";
import { libraryAction, type LibraryTrack } from "./library";
import { entryFromPath, entryFromTrack } from "./playlists";

export type ProviderId =
  | "local"
  | "youtube-auto"
  | "youtube"
  | "google-drive"
  | "dropbox"
  | "onedrive"
  | "webdav";
export type Capability =
  "auth" | "search" | "read" | "playback" | "download" | "revoke";
export type AuthKind = "none" | "api_key" | "oauth2" | "password";
export type SourceState =
  "disconnected" | "pending" | "connected" | "expired" | "revoked" | "error";

export interface ProviderSettings {
  client_id: string;
  endpoint: string;
  username: string;
}

export interface ProviderStatus {
  id: ProviderId;
  name: string;
  auth: AuthKind;
  capabilities: Capability[];
  required: string[];
  note: string;
  state: SourceState;
  label: string;
  endpoint: string;
  expires_at: number | null;
  last_error: string | null;
  has_secret: boolean;
  settings: ProviderSettings;
}

export interface SourceEntry {
  provider: ProviderId;
  id: string;
  name: string;
  kind: string;
  detail: string;
  size: number | null;
  source_url: string | null;
  artist?: string | null;
  album?: string | null;
  thumbnail?: string | null;
}

export interface SourcesSnapshot {
  providers: ProviderStatus[];
  results: SourceEntry[];
  external: boolean;
  pending_url: string | null;
  downloaded: string | null;
  warnings: string[];
}

export const emptySources: SourcesSnapshot = {
  providers: [],
  results: [],
  external: true,
  pending_url: null,
  downloaded: null,
  warnings: [],
};

export const sourcesAction = (action: object): Promise<SourcesSnapshot> =>
  invoke("sources_action", { action });

export async function preparedSourceEntry(path: string, source: SourceEntry) {
  const library = await libraryAction({ kind: "snapshot" });
  const track = library.tracks.find((track) => track.local_path === path);
  return track
    ? entryFromTrack(track)
    : {
        ...entryFromPath(path),
        title: source.name,
        artist: source.artist ?? null,
        album: source.album ?? null,
      };
}

export function sourceLibraryTrack(tracks: LibraryTrack[], entry: SourceEntry) {
  const providers =
    entry.provider === "youtube" || entry.provider === "youtube-auto"
      ? ["youtube", "youtube-auto"]
      : [entry.provider];
  return tracks.find(
    (track) =>
      (entry.provider === "local" && track.id === entry.id) ||
      providers.some(
        (provider) =>
          track.metadata?.identifiers.provider_track_ids?.[provider] ===
            entry.id ||
          (track.source === provider && track.source_id === entry.id),
      ),
  );
}

/** Prepare a playable local copy before saving an external result as a favorite. */
export async function favoriteSource(
  entry: SourceEntry,
  tracks: LibraryTrack[],
) {
  let track = sourceLibraryTrack(tracks, entry);
  if (!track) {
    const prepared = await sourcesAction({
      kind: "prepare",
      provider: entry.provider,
      id: entry.id,
    });
    if (!prepared.downloaded)
      throw new Error("No se pudo preparar el audio para favoritos");
    const snapshot = await libraryAction({ kind: "snapshot" });
    track = snapshot.tracks.find(
      (item) => item.local_path === prepared.downloaded,
    );
    if (!track) throw new Error("No se encontró el audio en la biblioteca");
  }
  return libraryAction({
    kind: "favorite",
    id: track.id,
    favorite: !track.favorite,
  });
}

/** Prefer the configured API for YouTube search, avoiding duplicate catalogs. */
export function searchableExternalProviders(snapshot: SourcesSnapshot) {
  if (!snapshot.external) return [];
  const providers = snapshot.providers.filter(
    (provider) =>
      provider.id !== "local" &&
      canQuery(provider) &&
      hasCapability(provider, "search"),
  );
  const hasYoutubeApi = providers.some((provider) => provider.id === "youtube");
  return providers.filter(
    (provider) => !hasYoutubeApi || provider.id !== "youtube-auto",
  );
}

export async function searchExternalSources(query: string) {
  let snapshot: SourcesSnapshot;
  try {
    snapshot = await sourcesAction({ kind: "snapshot" });
  } catch (reason) {
    return {
      providers: [],
      results: [],
      warnings: [String(reason)],
      external: true,
    };
  }
  const providers = searchableExternalProviders(snapshot);
  const results: SourceEntry[] = [];
  const warnings: string[] = [];
  // The native YouTube helper allows one operation at a time.
  for (const provider of providers) {
    try {
      const next = await sourcesAction({
        kind: "search",
        provider: provider.id,
        query,
      });
      results.push(...next.results);
      warnings.push(...next.warnings);
    } catch (reason) {
      warnings.push(`${provider.name}: ${String(reason)}`);
    }
  }
  return { providers, results, warnings, external: snapshot.external };
}

export const capabilityLabels: Record<Capability, string> = {
  auth: "Autorización",
  search: "Búsqueda",
  read: "Lectura",
  playback: "Reproducción",
  download: "Descarga",
  revoke: "Revocación",
};

export const stateLabels: Record<SourceState, string> = {
  disconnected: "Sin conectar",
  pending: "Autorizando",
  connected: "Conectada",
  expired: "Caducada",
  revoked: "Revocada",
  error: "Con error",
};

export const fieldLabels: Record<string, string> = {
  api_key: "Clave de la API",
  client_id: "ID de cliente",
  client_secret: "Clave del cliente (opcional)",
  endpoint: "Dirección",
  username: "Usuario",
  password: "Contraseña",
};

/** Connection fields are shown as one input kind each: secrets are masked,
 * addresses are validated by the browser before they are sent. */
export function fieldKind(field: string): "password" | "url" | "text" {
  if (field === "password" || field === "client_secret" || field === "api_key")
    return "password";
  if (field === "endpoint") return "url";
  return "text";
}

export function fieldLabel(field: string): string {
  return fieldLabels[field] ?? field;
}

export function hasCapability(
  provider: ProviderStatus,
  capability: Capability,
): boolean {
  return provider.capabilities.includes(capability);
}

/** Whether the connection form can be offered again for this provider. */
export function canConnect(provider: ProviderStatus): boolean {
  if (provider.id === "local" || provider.id === "youtube-auto") return false;
  return (
    provider.state === "disconnected" ||
    provider.state === "expired" ||
    provider.state === "revoked" ||
    provider.state === "error"
  );
}

export function canQuery(provider: ProviderStatus): boolean {
  return provider.id === "local" || provider.state === "connected";
}

export function canDisconnect(provider: ProviderStatus): boolean {
  return (
    provider.id !== "local" &&
    provider.id !== "youtube-auto" &&
    (provider.has_secret ||
      !["disconnected", "revoked"].includes(provider.state))
  );
}

export function entryActions(provider: ProviderStatus, entry: SourceEntry) {
  return {
    play:
      (entry.kind === "audio" ||
        (entry.provider === "youtube" && entry.kind === "video")) &&
      hasCapability(provider, "playback"),
    download: entry.kind !== "folder" && hasCapability(provider, "download"),
  };
}

export function stateHint(provider: ProviderStatus): string {
  if (provider.last_error) return provider.last_error;
  if (provider.state === "connected" && provider.label) return provider.label;
  if (provider.state === "connected" && provider.endpoint)
    return provider.endpoint;
  if (provider.state === "pending")
    return "Esperando la autorización en el navegador";
  return stateLabels[provider.state];
}

export function formatSize(size: number | null): string {
  if (size === null) return "";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}
