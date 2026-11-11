import { useState, type CSSProperties } from "react";
import type { AudioController } from "./audio";
import { Icon } from "./Icon";
import { TrackArtwork } from "./TrackArtwork";
import type { LibraryTrack } from "./library";
import { formatTime } from "./musicViews";
import type { AppView } from "./navigation";
export function PlayerBar({
  audio,
  current,
  navigate,
  onOpenFiles,
  onFavorite,
  expanded = false,
}: {
  audio: AudioController;
  current?: LibraryTrack;
  navigate: (view: AppView) => void;
  onOpenFiles: () => void;
  onFavorite: () => void;
  expanded?: boolean;
}) {
  const playback = audio.state;
  const track = playback.track;
  const hasQueue = playback.queue.length > 0;
  const isPlaying =
    playback.status === "playing" ||
    (playback.status === "loading" && playback.playWhenReady !== false);
  const canSeek =
    !!playback.durationMs && ["playing", "paused"].includes(playback.status);
  const seekMax = Math.max(playback.durationMs ?? 0, 1);
  const [seekValue, setSeekValue] = useState(0);
  const [seeking, setSeeking] = useState(false);
  const progress = Math.max(
    0,
    Math.min(
      100,
      ((seeking ? seekValue : playback.positionMs) / seekMax) * 100,
    ),
  );
  const bufferedBackground =
    (playback.bufferedRanges ?? [])
      .map((range) => {
        const start = Math.max(
          0,
          Math.min(100, (range.startMs / seekMax) * 100),
        );
        const end = Math.max(
          start,
          Math.min(100, (range.endMs / seekMax) * 100),
        );
        return `linear-gradient(90deg, transparent ${start}%, var(--buffered-audio) ${start}%, var(--buffered-audio) ${end}%, transparent ${end}%)`;
      })
      .join(", ") || "linear-gradient(transparent, transparent)";
  function cycleRepeat() {
    const modes = ["off", "all", "one"] as const;
    void audio.setRepeat(
      modes[(modes.indexOf(playback.repeat) + 1) % modes.length],
    );
  }
  const repeatLabel = (mode: string) =>
    mode === "all"
      ? "Repetir todo"
      : mode === "one"
        ? "Repetir una"
        : "Sin repetición";
  return (
    <footer
      className={`player-bar${expanded ? " player-controls-expanded" : ""}`}
      aria-label="Reproductor"
    >
      {expanded && (
        <TrackArtwork
          className="expanded-cover"
          trackId={current?.id ?? track?.trackId}
          path={track?.path}
          fallbackId={current?.artwork_id}
          thumbnail={track?.thumbnail}
          animate
          paused={!isPlaying}
        />
      )}
      <div className="now-playing">
        {!expanded && (
          <button
            className="cover-link"
            aria-label="Maximizar reproductor"
            aria-haspopup="dialog"
            onClick={() => navigate("nowPlaying")}
          >
            <TrackArtwork
              className="mini-artwork"
              trackId={current?.id ?? track?.trackId}
              path={track?.path}
              fallbackId={current?.artwork_id}
              thumbnail={track?.thumbnail}
            />
          </button>
        )}
        <div>
          {expanded ? (
            <strong className="expanded-track-title">
              {current?.title ?? track?.title ?? "Sin reproducción"}
            </strong>
          ) : (
            <button
              className="track-link"
              aria-haspopup="dialog"
              onClick={() => navigate("nowPlaying")}
            >
              <strong>{track?.title ?? "Sin reproducción"}</strong>
            </button>
          )}
          <span
            role={
              playback.buffering || playback.status === "loading"
                ? "status"
                : undefined
            }
          >
            {playback.status === "loading"
              ? "Conectando audio…"
              : playback.buffering && playback.status === "playing"
                ? "Almacenando en búfer…"
                : expanded && current
                  ? (current.artist ?? track?.artist ?? "Artista desconocido")
                  : track
                    ? [
                        track.format,
                        current?.artist ?? track.artist ?? "Archivo de audio",
                        current?.album ?? track.album,
                      ]
                        .filter(Boolean)
                        .join(" · ")
                    : audio.available
                      ? "Elige una canción"
                      : "Elige una canción"}
          </span>
        </div>
        <button
          className={`icon-button favorite-button ${current?.favorite ? "active" : ""}`}
          disabled={!audio.available || !track || current?.missing}
          aria-label={
            current?.favorite ? "Quitar de favoritos" : "Agregar a favoritos"
          }
          aria-pressed={current?.favorite ?? false}
          onClick={onFavorite}
        >
          <Icon name="heart" />
        </button>
      </div>
      <div className="transport">
        <div className="transport-buttons">
          <button
            className="icon-button"
            disabled={!audio.available || !hasQueue}
            aria-label="Canción anterior"
            onClick={() => void audio.previous()}
          >
            <Icon name="previous" size={18} />
          </button>
          <button
            className="play-button"
            disabled={!audio.available}
            aria-label={isPlaying ? "Pausar" : "Reproducir"}
            onClick={() => {
              if (!hasQueue) onOpenFiles();
              else void audio.toggle();
            }}
          >
            <Icon name={isPlaying ? "pause" : "play"} size={19} />
          </button>
          <button
            className="icon-button"
            disabled={!audio.available || !hasQueue}
            aria-label="Canción siguiente"
            onClick={() => void audio.next()}
          >
            <Icon name="next" size={18} />
          </button>
        </div>
        <div className="progress-row">
          <span>{formatTime(playback.positionMs)}</span>
          <input
            style={
              {
                "--progress": `${progress}%`,
                "--buffered-ranges": bufferedBackground,
              } as CSSProperties
            }
            type="range"
            aria-label="Posición de reproducción"
            aria-valuetext={`${formatTime(seeking ? seekValue : playback.positionMs)} de ${formatTime(playback.durationMs)}${playback.buffering ? ", almacenando en búfer" : ""}`}
            min="0"
            max={seekMax}
            step="1"
            value={Math.min(seeking ? seekValue : playback.positionMs, seekMax)}
            disabled={!canSeek}
            onChange={(event) => {
              setSeeking(true);
              setSeekValue(Number(event.target.value));
            }}
            onPointerDown={() => {
              setSeeking(true);
              setSeekValue(playback.positionMs);
            }}
            onPointerUp={(event) => {
              setSeeking(false);
              void audio.seek(Number(event.currentTarget.value));
            }}
            onPointerCancel={() => setSeeking(false)}
            onKeyUp={(event) => {
              setSeeking(false);
              void audio.seek(Number(event.currentTarget.value));
            }}
            onBlur={(event) => {
              if (seeking) {
                setSeeking(false);
                void audio.seek(Number(event.currentTarget.value));
              }
            }}
          />
          <span>
            {expanded && playback.durationMs !== null
              ? `−${formatTime(Math.max(0, playback.durationMs - playback.positionMs))}`
              : formatTime(playback.durationMs)}
          </span>
        </div>
      </div>
      <div className="player-tools">
        <div className="volume-control">
          <button
            className="icon-button"
            disabled={!audio.available}
            aria-label={playback.muted ? "Activar sonido" : "Silenciar"}
            title={playback.muted ? "Activar sonido" : "Silenciar"}
            onClick={() => void audio.setMuted(!playback.muted)}
          >
            <Icon name={playback.muted ? "volumeOff" : "volume"} size={18} />
          </button>
          <div className="volume-slider">
            <input
              type="range"
              aria-label="Volumen"
              min="0"
              max="100"
              value={Math.round((playback.muted ? 0 : playback.volume) * 100)}
              disabled={!audio.available}
              onChange={(event) => {
                const volume = Number(event.target.value) / 100;
                if (playback.muted) void audio.setMuted(false);
                void audio.setVolume(volume);
              }}
            />
          </div>
        </div>
        <button
          className={`icon-button ${playback.shuffle ? "active" : ""}`}
          aria-label="Aleatorio"
          aria-pressed={playback.shuffle}
          title="Aleatorio"
          disabled={!audio.available}
          onClick={() => void audio.setShuffle(!playback.shuffle)}
        >
          <Icon name="shuffle" size={18} />
        </button>
        <button
          className={`icon-button ${playback.repeat !== "off" ? "active" : ""}`}
          aria-label={`Repetir: ${repeatLabel(playback.repeat)}`}
          title={`Repetir: ${repeatLabel(playback.repeat)}`}
          disabled={!audio.available}
          onClick={cycleRepeat}
        >
          <Icon
            name={playback.repeat === "one" ? "repeatOne" : "repeat"}
            size={18}
          />
        </button>
        {!expanded && (
          <>
            <button
              className="icon-button"
              aria-label="Abrir letras"
              title="Letras"
              onClick={() => navigate("lyrics")}
            >
              <Icon name="lyrics" />
            </button>
            <button
              className="icon-button"
              aria-label="Abrir cola"
              title="Cola"
              onClick={() => navigate("queue")}
            >
              <Icon name="queue" />
            </button>
          </>
        )}
      </div>
    </footer>
  );
}
