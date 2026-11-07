import type Hls from "hls.js";

export function isAppleMotionUrl(value: string) {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && !url.username && !url.password && !url.port &&
      (url.hostname === "mvod.itunes.apple.com" || url.hostname.endsWith(".mzstatic.com")) &&
      /\.(m3u8|mp4)$/.test(url.pathname);
  } catch { return false; }
}

// Adapted from dashboard/src/lib/motion-playback.ts. This decorative video
// never changes the audio engine, media session, queue or music volume.
export function attachMotionVideo(video: HTMLVideoElement, url: string, {
  onReady, onFailure,
}: { onReady: () => void; onFailure: () => void }) {
  let disposed = false;
  let hls: Hls | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const clearTimer = () => { clearTimeout(timer); timer = undefined; };
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    clearTimer();
    video.removeEventListener("playing", ready);
    video.removeEventListener("waiting", waiting);
    video.removeEventListener("stalled", waiting);
    video.removeEventListener("error", fail);
    hls?.destroy();
    video.pause();
    video.removeAttribute("src");
    video.load();
  };
  const fail = () => { if (!disposed) { dispose(); onFailure(); } };
  const waiting = () => { if (timer === undefined) timer = setTimeout(fail, 12000); };
  const ready = () => { if (!disposed) { clearTimer(); onReady(); } };
  const play = () => { if (!disposed) void video.play().catch(() => {}); };
  if (!isAppleMotionUrl(url)) { onFailure(); return () => {}; }
  video.muted = true;
  video.defaultMuted = true;
  video.loop = true;
  video.playsInline = true;
  video.addEventListener("playing", ready);
  video.addEventListener("waiting", waiting);
  video.addEventListener("stalled", waiting);
  video.addEventListener("error", fail);
  waiting();
  const native = video.canPlayType("application/vnd.apple.mpegurl");
  const playNative = () => { if (!disposed) { video.src = url; play(); } };
  if (new URL(url).pathname.endsWith(".mp4") || native === "probably") playNative();
  else {
    void import("hls.js").then(({ default: HlsPlayer }) => {
      if (disposed) return;
      if (!HlsPlayer.isSupported()) { if (native) playNative(); else fail(); return; }
      hls = new HlsPlayer({
        enableWorker: true,
        capLevelToPlayerSize: true,
        startLevel: 0,
        maxBufferLength: 10,
        maxMaxBufferLength: 20,
      });
      hls.on(HlsPlayer.Events.ERROR, (_event, data) => { if (data.fatal) fail(); });
      hls.on(HlsPlayer.Events.MANIFEST_PARSED, play);
      hls.loadSource(url);
      hls.attachMedia(video);
    }).catch(() => { if (native) playNative(); else fail(); });
  }
  return dispose;
}
