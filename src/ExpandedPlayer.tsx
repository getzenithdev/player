import { useEffect, useRef, useState } from "react";
import { sourceFromPlayback } from "./sources";
import type { AudioController } from "./audio";
import type { LibraryTrack } from "./library";
import type { AppView } from "./navigation";
import { Icon } from "./Icon";
import { TrackArtwork } from "./TrackArtwork";
import { PlayerBar } from "./PlayerBar";
import { Lyrics } from "./LyricsPanel";
import { QueuePanel } from "./QueuePanel";
import { animateSpring } from "./motion";
import "./ExpandedPlayer.css";

const sourceLabels: Record<string, string> = {
  local: "Tu biblioteca",
  youtube: "YouTube",
  "youtube-auto": "YouTube",
  "google-drive": "Google Drive",
  dropbox: "Dropbox",
  onedrive: "OneDrive",
  webdav: "WebDAV",
};

export function ExpandedPlayer({
  audio,
  current,
  navigate,
  onOpenFiles,
  onFavorite,
  onMinimize,
  lightweightMotion = false,
}: {
  audio: AudioController;
  current?: LibraryTrack;
  navigate: (view: AppView) => void;
  onOpenFiles: () => void;
  onFavorite: () => void;
  onMinimize: () => void;
  lightweightMotion?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const motion = useRef<ReturnType<typeof animateSpring> | null>(null);
  const minimize = useRef(onMinimize);
  const closing = useRef(false);
  const [showQueue, setShowQueue] = useState(false);
  const track = audio.state.track;
  useEffect(() => {
    minimize.current = onMinimize;
  }, [onMinimize]);
  useEffect(() => {
    if (closing.current) {
      minimize.current();
      return;
    }
    const element = dialog.current;
    const opener = document.activeElement;
    if (element && opener instanceof HTMLElement) {
      const bounds = opener.getBoundingClientRect();
      element.style.transformOrigin = `${bounds.left + bounds.width / 2}px ${bounds.top + bounds.height / 2}px`;
    }
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = (progress: number) => {
      if (!element) return;
      element.style.opacity = String(progress);
      element.style.transform = lightweightMotion
        ? "none"
        : `translateY(${(1 - progress) * 12}px) scale(${0.985 + progress * 0.015})`;
    };
    update(preference.matches ? 1 : 0);
    element?.showModal();
    if (!preference.matches && element) {
      if (lightweightMotion) {
        let animation: Animation | null = null;
        let deadline: number | undefined;
        const stop = () => {
          window.clearTimeout(deadline);
          if (animation) {
            animation.onfinish = null;
            animation.cancel();
            animation = null;
          }
        };
        const to = (target: number) => {
          const opacity = getComputedStyle(element).opacity;
          stop();
          element.style.opacity = String(target);
          animation = element.animate([{ opacity }, { opacity: target }], {
            duration: 150,
            easing: "cubic-bezier(0.23, 1, 0.32, 1)",
          });
          const finish = () => {
            stop();
            if (target === 0) minimize.current();
          };
          animation.onfinish = finish;
          // Do not wait for a spring to settle when rendering falls behind.
          deadline = window.setTimeout(finish, 150);
        };
        motion.current = { to, stop };
        to(1);
      } else {
        motion.current = animateSpring(update, (value) => {
          if (value === 0) minimize.current();
        });
      }
    }
    const reduce = () => {
      if (!preference.matches) return;
      motion.current?.stop();
      motion.current = null;
      if (closing.current) minimize.current();
      else update(1);
    };
    preference.addEventListener("change", reduce);
    return () => {
      motion.current?.stop();
      motion.current = null;
      preference.removeEventListener("change", reduce);
      element?.close();
      if (opener instanceof HTMLElement && opener.isConnected)
        opener.focus({ preventScroll: true });
    };
  }, [lightweightMotion]);
  function closePlayer() {
    if (closing.current) return;
    closing.current = true;
    if (motion.current) motion.current.to(0);
    else onMinimize();
  }
  return (
    <dialog
      ref={dialog}
      className="expanded-player"
      aria-label="Reproductor expandido"
      onCancel={(event) => {
        event.preventDefault();
        closePlayer();
      }}
    >
      <TrackArtwork
        className="expanded-backdrop"
        trackId={current?.id ?? track?.trackId}
        path={track?.path}
        fallbackId={current?.artwork_id}
        thumbnail={track?.thumbnail}
      />
      <header className="expanded-header">
        <button
          autoFocus
          className="icon-button minimize-player"
          aria-label="Minimizar reproductor"
          title="Minimizar (Esc)"
          onClick={closePlayer}
        >
          <Icon name="minimize" size={24} />
        </button>
        <span>
          {sourceLabels[
            current?.source ??
              (track && sourceFromPlayback(track)?.provider) ??
              "local"
          ] ?? "Ahora suena"}
        </span>
        <div className="expanded-header-actions">
          <button
            className={`icon-button ${showQueue ? "active" : ""}`}
            aria-label={showQueue ? "Mostrar letras" : "Mostrar cola"}
            aria-pressed={showQueue}
            title={showQueue ? "Letras" : "Cola"}
            onClick={() => setShowQueue((value) => !value)}
          >
            <Icon name={showQueue ? "lyrics" : "queue"} />
          </button>
          <details className="expanded-options">
            <summary aria-label="Opciones de la canción">
              <Icon name="more" />
            </summary>
            <div>
              <button onClick={() => navigate("lyrics")}>
                Buscar o editar letras
              </button>
              <button onClick={() => navigate("artwork")}>
                Cambiar portada
              </button>
              <button
                disabled={!audio.available || !track}
                onClick={() => void audio.stop()}
              >
                Detener reproducción
              </button>
            </div>
          </details>
        </div>
      </header>
      <div className="expanded-body">
        <PlayerBar
          expanded
          audio={audio}
          current={current}
          navigate={navigate}
          onOpenFiles={onOpenFiles}
          onFavorite={onFavorite}
        />
        <section
          className={`expanded-reading${showQueue ? " showing-queue" : ""}`}
          aria-label={
            showQueue ? "Cola de reproducción" : "Letras de la canción"
          }
        >
          {showQueue ? (
            <QueuePanel audio={audio} />
          ) : (
            <Lyrics
              audio={audio}
              immersive
              onManage={() => navigate("lyrics")}
            />
          )}
        </section>
      </div>
      {(audio.error || audio.state.error) && (
        <p className="expanded-error" role="alert">
          {audio.error || audio.state.error}
        </p>
      )}
    </dialog>
  );
}
