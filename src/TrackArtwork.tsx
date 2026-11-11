import { memo, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import {
  discoverArtwork,
  loadArtwork,
  loadArtworkFrames,
  subscribeArtwork,
  type LoadedArtwork,
} from "./artworkClient";
import { attachMotionVideo, isAppleMotionUrl } from "./motionPlayback";
import { useAppVisibility } from "./useAppVisibility";
import {
  observeNearbyArtwork,
  observeVisibleArtwork,
} from "./artworkVisibility";
import "./TrackArtwork.css";

export const TrackArtwork = memo(function TrackArtwork({
  trackId,
  className = "resource-art",
  animate = false,
  paused = false,
  fallbackId,
  path,
  quality,
  thumbnail,
}: {
  trackId?: string | null;
  className?: string;
  animate?: boolean;
  paused?: boolean;
  fallbackId?: string | null;
  path?: string | null;
  quality?: "thumbnail" | "preview";
  thumbnail?: string | null;
}) {
  // The static image belongs to the song, independently of whether motion is
  // enabled or a different preview quality is being requested.
  const identity = JSON.stringify([trackId, path, fallbackId, thumbnail]);
  const [loaded, setLoaded] = useState<{ key: string; value: LoadedArtwork }>();
  // Render only resources belonging to this song; an old response never flashes
  // the preceding song's cover while its replacement loads.
  const data = loaded?.key === identity ? loaded.value : null;
  const asset = data?.snapshot.resolved;
  const resourceTrack = data?.trackId;
  const resourceIdentity = data?.snapshot.identity ?? resourceTrack ?? "";
  const appVisible = useAppVisibility();
  const [visible, setVisible] = useState(false);
  const [nearby, setNearby] = useState(false);
  const [reduced, setReduced] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const [ready, setReady] = useState("");
  const [failedVideo, setFailedVideo] = useState<{
    key: string;
    until: number;
  }>();
  const [failedPoster, setFailedPoster] = useState("");
  const [notice, setNotice] = useState<{ key: string; text: string }>();
  const [frames, setFrames] = useState<{
    key: string;
    items: { url: string; delay: number }[];
    index: number;
  }>();
  const holder = useRef<HTMLSpanElement>(null);
  const video = useRef<HTMLVideoElement>(null);
  const refreshedAt = useRef(0);
  const active = animate && !paused && appVisible && visible && !reduced;
  const motionUrl = asset?.motion_url;
  const videoKey = JSON.stringify([identity, resourceIdentity, motionUrl]);
  const showVideo =
    active &&
    !!motionUrl &&
    isAppleMotionUrl(motionUrl) &&
    failedVideo?.key !== videoKey;
  const gifKey = JSON.stringify([identity, asset?.id]);
  const currentFrames = frames?.key === gifKey ? frames : null;

  useEffect(() => {
    setFailedPoster("");
    setFrames(undefined);
    setReady("");
  }, [identity]);

  useEffect(() => {
    if (active) return;
    // Release only decoded motion resources. Keep the loaded static poster
    // while mounted, including when the window is hidden or scrolled away.
    const timer = window.setTimeout(() => {
      setFrames(undefined);
      setReady("");
    }, 15_000);
    return () => window.clearTimeout(timer);
  }, [active]);

  useEffect(() => {
    const element = holder.current;
    if (!element) return;
    const stopNearby = observeNearbyArtwork(element, setNearby);
    const stopVisible = animate
      ? observeVisibleArtwork(element, setVisible)
      : undefined;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const change = () => setReduced(preference.matches);
    preference.addEventListener("change", change);
    return () => {
      stopNearby();
      stopVisible?.();
      preference.removeEventListener("change", change);
    };
  }, [animate]);

  useEffect(() => {
    if (!nearby || !appVisible) return;
    let disposed = false;
    let version = 0;
    const reload = async () => {
      const request = ++version;
      try {
        const value = await loadArtwork(
          trackId,
          path,
          fallbackId,
          animate,
          quality,
        );
        if (!disposed && request === version)
          setLoaded({ key: identity, value });
      } catch (error) {
        // A refresh failure keeps the last valid poster of this same song.
        if (!disposed && request === version)
          setNotice({ key: identity, text: String(error) });
      }
    };
    const unsubscribe = subscribeArtwork(() => void reload());
    if (isTauri()) void reload();
    return () => {
      disposed = true;
      unsubscribe();
    };
  }, [
    identity,
    trackId,
    path,
    fallbackId,
    animate,
    quality,
    nearby,
    appVisible,
  ]);

  useEffect(() => {
    if (!active || !resourceTrack) return;
    let cancelled = false;
    void discoverArtwork(resourceTrack, resourceIdentity).catch((error) => {
      if (!cancelled) setNotice({ key: identity, text: String(error) });
    });
    return () => {
      cancelled = true;
    };
  }, [active, resourceTrack, resourceIdentity, identity]);

  useEffect(() => {
    const element = video.current;
    if (!showVideo || !motionUrl || !element) return;
    return attachMotionVideo(element, motionUrl, {
      onReady: () => setReady(videoKey),
      onFailure: () => {
        setFailedVideo({ key: videoKey, until: Date.now() + 60_000 });
        setNotice({
          key: identity,
          text: "No se pudo cargar el video de portada; se conserva la imagen y se reintentará.",
        });
        if (resourceTrack && Date.now() - refreshedAt.current >= 60_000) {
          refreshedAt.current = Date.now();
          void discoverArtwork(resourceTrack, resourceIdentity, true).catch(
            () => {},
          );
        }
      },
    });
  }, [
    showVideo,
    motionUrl,
    videoKey,
    identity,
    resourceTrack,
    resourceIdentity,
  ]);

  useEffect(() => {
    if (!failedVideo || !active) return;
    const timer = window.setTimeout(
      () => setFailedVideo(undefined),
      Math.max(0, failedVideo.until - Date.now()),
    );
    return () => window.clearTimeout(timer);
  }, [failedVideo, active]);

  useEffect(() => {
    if (!active || motionUrl || !asset?.animated || !asset.id.endsWith(".gif"))
      return;
    if (frames?.key === gifKey && frames.items.length) return;
    let cancelled = false;
    void loadArtworkFrames(asset.id)
      .then((items) => {
        if (!cancelled) setFrames({ key: gifKey, items, index: 0 });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [
    active,
    motionUrl,
    asset?.animated,
    asset?.id,
    gifKey,
    frames?.key,
    frames?.items.length,
  ]);

  useEffect(() => {
    if (!active || !currentFrames?.items.length) return;
    const timer = window.setTimeout(
      () =>
        setFrames((previous) =>
          previous?.key === gifKey
            ? {
                ...previous,
                index: (previous.index + 1) % previous.items.length,
              }
            : previous,
        ),
      currentFrames.items[currentFrames.index]?.delay ?? 100,
    );
    return () => window.clearTimeout(timer);
  }, [active, currentFrames, gifKey]);

  const poster = data?.poster || thumbnail;
  const image =
    active && currentFrames
      ? (currentFrames.items[currentFrames.index]?.url ?? poster)
      : poster;
  const title =
    notice?.key === identity
      ? notice.text
      : data?.snapshot.warnings.join(" · ");
  return (
    <span
      ref={holder}
      className={`track-artwork ${className}${poster ? "" : " resource-art-empty"}`}
      title={title || undefined}
      style={{ display: "inline-block", overflow: "hidden" }}
    >
      {image && failedPoster !== image ? (
        <img
          src={image}
          decoding="async"
          onError={() => setFailedPoster(image)}
          alt=""
          style={{ width: "100%", height: "100%", objectFit: "cover" }}
        />
      ) : (
        <span aria-hidden="true">
          <img className="fallback-mark" src="/favicon.svg" alt="" />
        </span>
      )}
      {showVideo && (
        <video
          key={videoKey}
          ref={video}
          muted
          loop
          playsInline
          preload="none"
          disablePictureInPicture
          disableRemotePlayback
          aria-hidden="true"
          tabIndex={-1}
          style={{
            position: "absolute",
            inset: 0,
            width: "100%",
            height: "100%",
            objectFit: "cover",
            pointerEvents: "none",
            opacity: ready === videoKey ? 1 : 0,
          }}
        />
      )}
    </span>
  );
});
