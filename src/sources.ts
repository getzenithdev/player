import { invoke } from "@tauri-apps/api/core";

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
  external: false,
  pending_url: null,
  downloaded: null,
  warnings: [],
};

export const sourcesAction = (action: object): Promise<SourcesSnapshot> =>
  invoke("sources_action", { action });

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
    play: entry.kind === "audio" && hasCapability(provider, "playback"),
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
