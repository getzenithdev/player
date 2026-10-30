import { invoke, isTauri } from "@tauri-apps/api/core";

export interface DocumentStatus {
  path: string;
  exists: boolean;
  version: number | null;
  valid: boolean;
  backups: number;
}

export interface RecoveryNotice {
  file: string;
  /// "recovered", "preserved" o "index".
  kind: string;
  message: string;
}

export interface IndexStatus {
  fields: string[];
  tracks: number;
}

export interface StorageStatus {
  root: string;
  portable: boolean;
  writable: boolean;
  error: string | null;
  schema_version: number;
  directories_ready: boolean;
  documents: DocumentStatus[];
  recovery: RecoveryNotice[];
  index: IndexStatus;
  cache_bytes: number;
}

export interface BackupReport {
  created: string[];
  skipped: number;
  directory: string;
}

export interface ExportReport {
  path: string;
  config: number;
  library: number;
  lyrics: number;
  excluded: string[];
  skipped: string[];
}

export interface ImportReport {
  path: string;
  config: number;
  library: number;
  lyrics: number;
  skipped: string[];
}

export interface CacheReport {
  removed_files: number;
  freed_bytes: number;
  errors: string[];
}

export interface AppearanceData {
  theme: string;
}

export interface TrackHit {
  id: string;
  title: string;
  artist: string | null;
  album: string | null;
  path: string | null;
}

/// Message used when an action needs the desktop application.
export const nativeOnlyMessage =
  "El almacenamiento local está disponible en la aplicación de escritorio.";

/** True when the desktop storage commands can be reached. */
export function hasNativeStorage(): boolean {
  return isTauri();
}

/**
 * Storage snapshot for the settings screen, or `null` when the interface runs
 * in a browser without the Rust side.
 */
export async function loadStorageStatus(): Promise<StorageStatus | null> {
  if (!isTauri()) return null;
  return invoke<StorageStatus>("storage_status");
}

/** Stored appearance preferences, or `null` outside the desktop runtime. */
export async function loadAppearance(): Promise<AppearanceData | null> {
  if (!isTauri()) return null;
  return invoke<AppearanceData>("appearance_get");
}

/**
 * Persists one of the accepted themes. Returns `false` outside the desktop
 * runtime so the caller keeps the preview-only behaviour instead of failing.
 */
export async function saveAppearance(theme: string): Promise<boolean> {
  if (!isTauri()) return false;
  await invoke<AppearanceData>("appearance_save", { theme });
  return true;
}

/** Copies every stored document into `backups/`. */
export async function createBackup(): Promise<BackupReport> {
  if (!isTauri()) throw new Error(nativeOnlyMessage);
  return invoke<BackupReport>("storage_backup");
}

/** Writes an export bundle without credentials and reports its location. */
export async function exportData(): Promise<ExportReport> {
  if (!isTauri()) throw new Error(nativeOnlyMessage);
  return invoke<ExportReport>("storage_export");
}

/**
 * Restores a previously exported bundle. Without a path the newest export in
 * `backups/` is used.
 */
export async function importData(path?: string): Promise<ImportReport> {
  if (!isTauri()) throw new Error(nativeOnlyMessage);
  return invoke<ImportReport>("storage_import", { path: path ?? null });
}

/** Empties the cache directories without touching documents or music. */
export async function clearCache(): Promise<CacheReport> {
  if (!isTauri()) throw new Error(nativeOnlyMessage);
  return invoke<CacheReport>("storage_clear_cache");
}

/** Case-insensitive search over the in-memory indexes. */
export async function searchLibrary(query: string): Promise<TrackHit[]> {
  if (!isTauri()) return [];
  return invoke<TrackHit[]>("storage_search", { query });
}

/** Human-readable byte size used by the storage screen. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const decimals = unit > 0 && value % 1 !== 0 ? 1 : 0;
  return `${value.toFixed(decimals)} ${units[unit]}`;
}
