import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { artworkAction, emptyArtwork, type ArtworkSnapshot } from "./artwork";
import { libraryAction } from "./library";

export interface LoadedArtwork {
  trackId: string | null;
  snapshot: ArtworkSnapshot;
  poster: string;
}
const cache = new Map<string, { value: LoadedArtwork; expires: number }>();
const pending = new Map<string, Promise<LoadedArtwork>>();
const discoveries = new Map<string, Promise<ArtworkSnapshot>>();
const listeners = new Set<() => void>();
let revision = 0;
let subscribed = false;

function invalidate() {
  ++revision;
  cache.clear();
  pending.clear();
  listeners.forEach((listener) => listener());
}

export function subscribeArtwork(listener: () => void) {
  listeners.add(listener);
  if (!subscribed && isTauri()) {
    subscribed = true;
    void Promise.all([
      listen("artwork:changed", invalidate),
      listen("library:changed", invalidate),
    ])
      .then(() => invalidate())
      .catch(() => {
        subscribed = false;
      });
  }
  return () => {
    listeners.delete(listener);
  };
}

export function loadArtwork(
  trackId: string | null | undefined,
  path: string | null | undefined,
  fallbackId: string | null | undefined,
  animate: boolean,
): Promise<LoadedArtwork> {
  const key = JSON.stringify([trackId, path, fallbackId, animate]);
  const saved = cache.get(key);
  if (saved && saved.expires > Date.now()) {
    // Recently viewed artwork stays cached when a long list is scrolled back.
    cache.delete(key);
    cache.set(key, saved);
    return Promise.resolve(saved.value);
  }
  if (saved) cache.delete(key);
  const existing = pending.get(key);
  if (existing) return existing;
  const version = revision;
  const task = async () => {
    const resolvedId =
      trackId ||
      (path
        ? (await libraryAction({ kind: "snapshot" })).tracks.find(
            (track) => track.local_path === path,
          )?.id
        : null);
    const snapshot = resolvedId
      ? await artworkAction(resolvedId, { kind: "snapshot" })
      : emptyArtwork;
    let poster = "";
    if (snapshot.resolved) {
      try {
        poster = await invoke<string>("artwork_data", {
          id: snapshot.resolved.id,
          variant: animate ? snapshot.preferences.quality : "thumbnail",
        });
      } catch {
        /* Keep the embedded/local cover if the selected resource is unavailable. */
      }
    }
    if (!poster && fallbackId)
      poster = await invoke<string>("library_artwork", { id: fallbackId });
    const value = { trackId: resolvedId || null, snapshot, poster };
    if (version === revision) {
      if (cache.size >= 16) cache.delete(cache.keys().next().value!);
      cache.set(key, { value, expires: Date.now() + 5 * 60_000 });
    }
    return value;
  };
  const request: Promise<LoadedArtwork> = task().finally(() => {
    if (pending.get(key) === request) pending.delete(key);
  });
  pending.set(key, request);
  return request;
}

export function discoverArtwork(
  trackId: string,
  identity: string,
  force = false,
) {
  const key = JSON.stringify([trackId, identity, force]);
  const existing = discoveries.get(key);
  if (existing) return existing;
  const request: Promise<ArtworkSnapshot> = artworkAction(trackId, {
    kind: "discover",
    force,
  }).finally(() => {
    if (discoveries.get(key) === request) discoveries.delete(key);
  });
  discoveries.set(key, request);
  return request;
}
