import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type { LibraryTrack } from "./library";
export interface MediaEntry {
  id: string;
  trackId: string | null;
  path: string;
  title: string;
  format: string;
  durationMs: number | null;
  available: boolean;
}
export interface Playlist {
  id: string;
  name: string;
  entries: MediaEntry[];
  updated_at: number;
}
export interface PlaylistSnapshot {
  playlists: Playlist[];
  notice: string | null;
}
export type PlaylistAction =
  | { kind: "snapshot" }
  | { kind: "create"; name: string; entries?: MediaEntry[] }
  | { kind: "rename"; id: string; name: string }
  | { kind: "duplicate" | "delete"; id: string }
  | { kind: "add"; id: string; entries: MediaEntry[] }
  | { kind: "remove"; id: string; entry_id: string }
  | { kind: "move"; id: string; entry_id: string; before_id: string | null }
  | { kind: "import"; path: string }
  | { kind: "export"; id: string; path: string; format: string };
export type QueueAction =
  | { kind: "append"; paths: string[]; after_current: boolean }
  | { kind: "remove"; id: string }
  | { kind: "move"; id: string; before_id: string | null }
  | { kind: "save" | "restore" };
export interface HistoryVisit {
  id: string;
  entry: MediaEntry;
  startedAt: number;
  updatedAt: number;
  listenedMs: number;
  positionMs: number;
}
export function entryFromTrack(t: LibraryTrack): MediaEntry {
  return {
    id: "",
    trackId: t.id,
    path: t.local_path ?? "",
    title: t.title,
    format: t.format,
    durationMs: t.duration === null ? null : t.duration * 1000,
    available: !t.missing,
  };
}
export function entryFromPath(path: string): MediaEntry {
  return {
    id: "",
    trackId: null,
    path,
    title: path.split(/[\\/]/).pop() ?? path,
    format: "",
    durationMs: null,
    available: true,
  };
}
export function moveTarget(
  entries: MediaEntry[],
  index: number,
  direction: -1 | 1,
): string | null | undefined {
  if (direction === -1) return index > 0 ? entries[index - 1].id : undefined;
  return index < entries.length - 1
    ? (entries[index + 2]?.id ?? null)
    : undefined;
}
export function usePlaylists() {
  const [snapshot, setSnapshot] = useState<PlaylistSnapshot>({
    playlists: [],
    notice: null,
  });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const alive = useRef(false);
  const generation = useRef(0);
  const reload = useCallback(async () => {
    const request = ++generation.current;
    try {
      const result = await invoke<PlaylistSnapshot>("playlist_action", {
        action: { kind: "snapshot" },
      });
      if (alive.current && request === generation.current) setSnapshot(result);
    } catch (reason) {
      if (alive.current) setError(String(reason));
    }
  }, []);
  useEffect(() => {
    alive.current = true;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    if (isTauri())
      void (async () => {
        const cleanup = await listen("playlists:changed", () => {
          void reload();
        });
        if (disposed) cleanup();
        else unlisten = cleanup;
        if (!disposed) await reload();
      })().catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
      alive.current = false;
      unlisten?.();
    };
  }, [reload]);
  const run = useCallback(async (action: PlaylistAction) => {
    if (!isTauri()) return null;
    setBusy(true);
    setError(null);
    ++generation.current;
    try {
      const result = await invoke<PlaylistSnapshot>("playlist_action", {
        action,
      });
      if (alive.current) {
        ++generation.current;
        setSnapshot(result);
      }
      return result;
    } catch (reason) {
      if (alive.current) setError(String(reason));
      return null;
    } finally {
      if (alive.current) setBusy(false);
    }
  }, []);
  return { ...snapshot, error, busy, run };
}
