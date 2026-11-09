import { memo, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { observeNearbyArtwork } from "./artworkVisibility";
import { ImageMemoryCache } from "./imageMemory";
import { useAppVisibility } from "./useAppVisibility";

const cache = new ImageMemoryCache<string | null>(
  6 * 1024 * 1024,
  48,
  (image) => (image?.length ?? 0) * 2 + 128,
);
const pending = new Map<string, Promise<string | null>>();
const revisions = new Map<string, number>();
const listeners = new Map<string, Set<(image: string | null) => void>>();
const queuedPhotos: Array<() => void> = [];
let activePhotoRequests = 0;

function requestPhoto(args: {
  artistKey: string;
  trackId: string | null;
  path: null;
  artistName: string | null;
}) {
  return new Promise<string | null>((resolve, reject) => {
    const start = () => {
      ++activePhotoRequests;
      void invoke<string | null>("artist_artwork", args)
        .then(resolve, reject)
        .finally(() => {
          --activePhotoRequests;
          queuedPhotos.shift()?.();
        });
    };
    if (activePhotoRequests < 3) start();
    else queuedPhotos.push(start);
  });
}

function remember(key: string, image: string | null) {
  cache.set(key, image, image ? 30 * 60_000 : 60_000);
  listeners.get(key)?.forEach((notify) => notify(image));
}

function loadPhoto(artistKey: string, trackId?: string, artistName?: string) {
  const saved = cache.get(artistKey);
  if (saved !== undefined) return Promise.resolve(saved);
  const existing = pending.get(artistKey);
  if (existing) return existing;
  const revision = revisions.get(artistKey) ?? 0;
  const request = requestPhoto({
    artistKey,
    trackId: trackId ?? null,
    path: null,
    artistName: artistName ?? null,
  })
    .then((image) => {
      if ((revisions.get(artistKey) ?? 0) === revision)
        remember(artistKey, image);
      return image;
    })
    .finally(() => pending.delete(artistKey));
  pending.set(artistKey, request);
  return request;
}

export function useArtistPhoto(
  artistKey: string,
  trackId?: string,
  enabled = true,
  artistName?: string,
) {
  const [photo, setPhoto] = useState<{ key: string; image: string | null }>();
  const appVisible = useAppVisibility();
  const active = enabled && appVisible;
  useEffect(() => {
    if (active) return;
    const timer = window.setTimeout(() => setPhoto(undefined), 15_000);
    return () => window.clearTimeout(timer);
  }, [active]);
  useEffect(() => {
    if (!active || !artistKey || !isTauri()) return;
    const notify = (image: string | null) =>
      setPhoto({ key: artistKey, image });
    const subscribers = listeners.get(artistKey) ?? new Set();
    listeners.set(artistKey, subscribers);
    subscribers.add(notify);
    const saved = cache.get(artistKey);
    if (saved !== undefined) notify(saved);
    const revision = revisions.get(artistKey) ?? 0;
    let disposed = false;
    // Oversized images can be used by a visible portrait without staying cached.
    void loadPhoto(artistKey, trackId, artistName)
      .then((image) => {
        if (!disposed && (revisions.get(artistKey) ?? 0) === revision)
          notify(image);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      subscribers.delete(notify);
      if (!subscribers.size) listeners.delete(artistKey);
    };
  }, [artistKey, trackId, active, artistName]);
  return photo?.key === artistKey ? photo.image : null;
}

export async function chooseArtistPhoto(artistKey: string, trackId?: string) {
  const path = await open({
    title: "Elegir foto del artista",
    multiple: false,
    filters: [{ name: "Imágenes", extensions: ["jpg", "jpeg", "png", "webp"] }],
  });
  if (typeof path !== "string") return;
  revisions.set(artistKey, (revisions.get(artistKey) ?? 0) + 1);
  const image = await invoke<string | null>("artist_artwork", {
    artistKey,
    trackId: trackId ?? null,
    path,
    artistName: null,
  });
  remember(artistKey, image);
}
export function forgetArtistPhotos() {
  cache.clear();
}

export const ArtistArtwork = memo(function ArtistArtwork({
  artistKey,
  name,
  trackId,
  className = "artist-portrait",
}: {
  artistKey: string;
  name: string;
  trackId?: string;
  className?: string;
}) {
  const holder = useRef<HTMLSpanElement>(null);
  const [nearby, setNearby] = useState(false);
  const image = useArtistPhoto(artistKey, trackId, nearby, name);
  const [failed, setFailed] = useState<string>();
  useEffect(() => {
    setFailed(undefined);
  }, [image]);
  useEffect(() => {
    if (holder.current) return observeNearbyArtwork(holder.current, setNearby);
  }, []);
  return (
    <span ref={holder} className={`artist-portrait ${className}`}>
      {image && failed !== image ? (
        <img
          src={image}
          alt=""
          decoding="async"
          onError={() => setFailed(image)}
        />
      ) : (
        <span className="artist-initials" aria-hidden="true">
          {name
            .trim()
            .split(/\s+/u)
            .slice(0, 2)
            .map((word) => Array.from(word)[0])
            .join("")
            .toLocaleUpperCase()}
        </span>
      )}
    </span>
  );
});
