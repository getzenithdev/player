import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { libraryAction } from "./library";
import { listen } from "@tauri-apps/api/event";
import {
  artworkAction,
  emptyArtwork,
  shouldAnimate,
  type ArtworkPreferences,
} from "./artwork";
export function TrackArtwork({
  trackId,
  className = "resource-art",
  animate = false,
  paused = false,
  fallbackId,
  path,
}: {
  trackId?: string | null;
  className?: string;
  animate?: boolean;
  paused?: boolean;
  fallbackId?: string | null;
  path?: string | null;
}) {
  const [data, setData] = useState<{
    id: string;
    url: string;
    animated: boolean;
    preferences: ArtworkPreferences;
  } | null>(null);
  const [frames, setFrames] = useState<{ url: string; delay: number }[]>([]);
  const [frame, setFrame] = useState(0);
  const [visible, setVisible] = useState(false);
  const [pageVisible, setPageVisible] = useState(!document.hidden);
  const [reduced, setReduced] = useState(
    window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const holder = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const node = holder.current;
    const observer = new IntersectionObserver((entries) =>
      setVisible(entries.some((e) => e.isIntersecting)),
    );
    if (node) observer.observe(node);
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const motion = () => setReduced(media.matches);
    const visibility = () => setPageVisible(!document.hidden);
    media.addEventListener("change", motion);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      observer.disconnect();
      media.removeEventListener("change", motion);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, []);
  useEffect(() => {
    let disposed = false;
    let request = 0;
    const load = async () => {
      const token = ++request;
      try {
        const id =
          trackId ||
          (path
            ? libraryAction({ kind: "snapshot" }).then(
                (s) => s.tracks.find((t) => t.local_path === path)?.id,
              )
            : null);
        const resolvedId = await id;
        const snapshot = resolvedId
          ? await artworkAction(resolvedId, { kind: "snapshot" })
          : emptyArtwork;
        let url = "";
        if (snapshot.resolved)
          url = await invoke("artwork_data", {
            id: snapshot.resolved.id,
            variant: animate ? snapshot.preferences.quality : "thumbnail",
          });
        else if (fallbackId)
          url = await invoke("library_artwork", { id: fallbackId });
        if (!disposed && token === request) {
          setData(
            url
              ? {
                  id: snapshot.resolved?.id ?? "",
                  url,
                  animated: snapshot.resolved?.animated ?? false,
                  preferences: snapshot.preferences,
                }
              : null,
          );
          setFrames([]);
          setFrame(0);
        }
      } catch {
        if (!disposed && token === request) setData(null);
      }
    };
    if (isTauri()) void load();
    const clean = isTauri()
      ? listen("artwork:changed", () => void load())
      : Promise.resolve(() => {});
    return () => {
      disposed = true;
      void clean.then((f) => f());
    };
  }, [trackId, fallbackId, path, animate]);
  const playing =
    !!data?.animated &&
    animate &&
    shouldAnimate(data.preferences, reduced, visible, pageVisible, paused);
  useEffect(() => {
    let disposed = false;
    if (playing && data && frames.length === 0)
      void invoke<{ url: string; delay: number }[]>("artwork_frames", {
        id: data.id,
      })
        .then((f) => {
          if (!disposed) setFrames(f);
        })
        .catch(() => {});
    return () => {
      disposed = true;
    };
  }, [playing, data, frames.length]);
  useEffect(() => {
    if (
      !playing ||
      !frames.length ||
      (!data?.preferences.looping && frame >= frames.length - 1)
    )
      return;
    const timer = window.setTimeout(
      () => setFrame((f) => (f + 1) % frames.length),
      frames[frame]?.delay ?? 100,
    );
    return () => window.clearTimeout(timer);
  }, [playing, frame, frames, data?.preferences.looping]);
  return (
    <span
      ref={holder}
      className={`${className}${data ? "" : " resource-art-empty"}`}
      style={{ display: "inline-block", overflow: "hidden" }}
    >
      {data ? (
        <img
          src={playing ? (frames[frame]?.url ?? data.url) : data.url}
          onError={() => setData(null)}
          alt=""
          style={{ width: "100%", height: "100%", objectFit: "cover" }}
        />
      ) : (
        <span aria-hidden="true">♫</span>
      )}
    </span>
  );
}
