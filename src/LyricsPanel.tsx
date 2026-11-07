import { useEffect, useRef, useState, type CSSProperties } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { AudioController } from "./audio";
import {
  activeLine,
  activeWord,
  defaultLyrics,
  discoverLyrics,
  displayedLyrics,
  lyricsAction,
  type LyricsPreferences,
  type LyricsSnapshot,
} from "./lyrics";
import { useResourceTrack } from "./ResourceTrack";
import "./Resources.css";
import { TrackArtwork } from "./TrackArtwork";

// CSS cubic-bezier(0.77, 0, 0.175, 1), shared by each scroll retarget.
function lyricScrollEase(progress: number) {
  if (progress <= 0 || progress >= 1) return progress;
  let low = 0;
  let high = 1;
  for (let i = 0; i < 16; i++) {
    const t = (low + high) / 2;
    const x = 3 * (1 - t) ** 2 * t * 0.77 +
      3 * (1 - t) * t ** 2 * 0.175 + t ** 3;
    if (x < progress) low = t;
    else high = t;
  }
  const t = (low + high) / 2;
  return 3 * (1 - t) * t ** 2 + t ** 3;
}

export function Lyrics({
  audio,
  compact = false,
  immersive = false,
  onManage,
}: {
  audio: AudioController;
  compact?: boolean;
  immersive?: boolean;
  onManage?: () => void;
}) {
  const { track, error: trackError, selector } = useResourceTrack(audio);
  const [snapshot, setSnapshot] = useState<LyricsSnapshot>(defaultLyrics);
  const [loadedTrackId, setLoadedTrackId] = useState<string>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [editor, setEditor] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [following, setFollowing] = useState(true);
  const [syncRequest, setSyncRequest] = useState(0);
  const section = useRef<HTMLElement>(null);
  const active = useRef<HTMLParagraphElement>(null);
  const generation = useRef(0);
  const trackId = track?.id;
  const trackMetadata = JSON.stringify([track?.title, track?.artist, track?.album, track?.duration]);
  useEffect(() => {
    setFollowing(true);
  }, [trackId]);
  useEffect(() => {
    const requests = generation;
    const token = ++requests.current;
    if (trackId)
      void lyricsAction(trackId, { kind: "snapshot" })
        .then(async (s) => {
          if (token === generation.current) {
            setSnapshot(s);
            setLoadedTrackId(trackId);
            setEditor(null);
            setError("");
            setBusy(false);
            if (!displayedLyrics(s) && !s.saved.suppress_local && s.preferences.external) {
              setBusy(true);
              try {
                const discovered = await discoverLyrics(trackId, trackMetadata);
                if (token === generation.current) {
                  setSnapshot({ ...s, ...discovered });
                }
              } catch (reason) {
                if (token === generation.current) setError(String(reason));
              } finally {
                if (token === generation.current) setBusy(false);
              }
            }
          }
        })
        .catch((e) => {
          if (token === generation.current) setError(String(e));
        });
    return () => {
      ++requests.current;
    };
  }, [trackId, trackMetadata]);
  useEffect(() => {
    const change = () =>
      setFullscreen(document.fullscreenElement === section.current);
    document.addEventListener("fullscreenchange", change);
    return () => document.removeEventListener("fullscreenchange", change);
  }, []);
  const candidate =
    loadedTrackId !== trackId
      ? null
      : displayedLyrics(snapshot);
  const preferences = snapshot.preferences;
  const isPlayingTrack = !!track &&
    (audio.state.track?.trackId === track.id ||
      audio.state.track?.path === track.local_path);
  const position = isPlayingTrack ? audio.state.positionMs : 0;
  const canSeek = audio.available && isPlayingTrack &&
    ["playing", "paused"].includes(audio.state.status);
  const offset =
    preferences.offset +
    snapshot.saved.offset +
    (candidate?.parsed.offset ?? 0);
  const line = activeLine(candidate?.parsed.lines ?? [], position, offset);
  useEffect(() => {
    if (!following) return;
    const node = active.current;
    const pane = node?.parentElement;
    if (node && pane) {
      const rect = node.getBoundingClientRect();
      const parent = pane.getBoundingClientRect();
      const start = pane.scrollTop;
      const target = Math.max(0, Math.min(pane.scrollHeight - pane.clientHeight,
          pane.scrollTop +
          rect.top -
          parent.top -
          pane.clientHeight / 2 +
          rect.height / 2));
      const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
      if (preference.matches) {
        pane.scrollTop = target;
        return;
      }
      // Retarget from the current scroll position when a line changes or a seek interrupts.
      let frame = 0;
      const started = performance.now();
      const step = (now: number) => {
        const progress = Math.min(1, (now - started) / 280);
        const eased = lyricScrollEase(progress);
        pane.scrollTop = start + (target - start) * eased;
        if (progress < 1) frame = requestAnimationFrame(step);
      };
      const reduce = () => {
        if (preference.matches) {
          cancelAnimationFrame(frame);
          pane.scrollTop = target;
        }
      };
      preference.addEventListener("change", reduce);
      frame = requestAnimationFrame(step);
      return () => {
        cancelAnimationFrame(frame);
        preference.removeEventListener("change", reduce);
      };
    }
  }, [line, candidate, following, syncRequest]);
  function resync() {
    setFollowing(true);
    setSyncRequest((value) => value + 1);
  }
  async function seekLine(at: number) {
    if (!canSeek) return;
    try {
      const target = Math.max(0, at - offset);
      await audio.seek(audio.state.durationMs === null
        ? target : Math.min(target, audio.state.durationMs));
      resync();
    } catch (reason) {
      audio.reportError(reason);
    }
  }
  async function run(action: { kind: string; [key: string]: unknown }) {
    if (!trackId) return;
    const token = ++generation.current;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const s = await lyricsAction(trackId, action);
      if (token === generation.current) {
        if (action.kind === "preferences") {
          s.candidates = snapshot.candidates;
        }
        setSnapshot(s);
        setLoadedTrackId(trackId);
        setNotice("Listo");
        return s;
      }
    } catch (e) {
      if (token === generation.current) setError(String(e));
    } finally {
      if (token === generation.current) setBusy(false);
    }
  }
  function settings(
    patch: Partial<LyricsPreferences>,
    trackOffset = snapshot.saved.offset,
  ) {
    const token = generation.current + 1;
    void run({
      kind: "preferences",
      preferences: { ...preferences, ...patch },
      offset: trackOffset,
    }).then((saved) => {
      if (token === generation.current && patch.external === true && saved?.preferences.external) {
        void run({ kind: "search", query: null });
      }
    });
  }
  async function importFile(plus = false) {
    try {
      const path = await open({
        multiple: false,
        filters: [
          {
            name: plus ? "LyricsPlus v2" : "Letra",
            extensions: plus ? ["json"] : ["lrc", "txt"],
          },
        ],
      });
      if (typeof path === "string")
        await run({ kind: plus ? "importPlus" : "import", path });
    } catch (e) {
      setError(String(e));
    }
  }
  async function exportFile() {
    try {
      const path = await save({
        defaultPath: "lyrics.lrc",
        filters: [{ name: "Letras", extensions: ["lrc", "txt"] }],
      });
      if (path) await run({ kind: "export", path });
    } catch (e) {
      setError(String(e));
    }
  }
  async function mode(mode: LyricsPreferences["mode"]) {
    if (mode === "fullscreen") {
      try {
        await section.current?.requestFullscreen();
      } catch {
        setError("No se pudo abrir la pantalla completa");
        return;
      }
    } else if (document.fullscreenElement) await document.exitFullscreen();
    settings({ mode });
  }
  const reading = (
    <div
      className={`lyric-lines${candidate ? "" : " lyrics-empty"}`}
      aria-label="Letra"
      tabIndex={0}
      onWheel={() => setFollowing(false)}
      onTouchMove={() => setFollowing(false)}
      onKeyDown={(event) => {
        if (["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End", " "].includes(event.key)
          && event.target === event.currentTarget) setFollowing(false);
      }}
    >
      {candidate ? (
        candidate.parsed.lines.map((l, i) => (
          <p
            key={i}
            ref={i === line ? active : undefined}
            className={i === line ? "active" : i < line ? "past" : ""}
            aria-current={i === line ? "true" : undefined}
          >
            <button
              type="button"
              className="lyric-line-button"
              disabled={l.at === null || !canSeek}
              onClick={() => { if (l.at !== null) void seekLine(l.at); }}
              aria-label={l.at === null ? undefined : `Ir a ${Math.floor(Math.max(0, l.at - offset) / 1000)} segundos: ${l.text || "♪"}`}
            >
            {preferences.mode === "karaoke" && l.words.length
              ? l.words.map((w, j) => (
                  <span
                    className={
                      j <= activeWord(l.words, position, offset) ? "sung" : ""
                    }
                    key={j}
                  >
                    {w.text}
                  </span>
                ))
              : l.text || "♪"}
            </button>
          </p>
        ))
      ) : (
        <div className="resource-empty">
          {busy ? "Buscando letras…" : "Sin letras disponibles."}
          {!busy && !preferences.external && (
            <p>Activa LRCLIB en las opciones de letras para buscar en línea.</p>
          )}
          {immersive && onManage && (
            <button className="button" onClick={onManage}>
              Buscar letras
            </button>
          )}
        </div>
      )}
    </div>
  );
  const syncButton = candidate?.parsed.lines.some((l) => l.at !== null) && (
    <div className="lyrics-sync-controls">
      <button type="button" className="button lyrics-resync" onClick={resync}
        aria-pressed={following}>
        Volver a sincronizar
      </button>
    </div>
  );
  if (immersive)
    return (
      <section
        className={`resources lyrics-engine lyrics-immersive${preferences.mode === "karaoke" ? " immersive-karaoke" : ""}`}
        style={
          {
            "--lyric-color": preferences.color,
            "--lyric-active": preferences.active_color,
            "--lyric-size": `${preferences.size}px`,
            fontFamily:
              preferences.font === "system" ? "inherit" : preferences.font,
          } as CSSProperties
        }
      >
        {!track ? (
          <div className="immersive-lyrics-empty">
            <p>Sin letras</p>
            <span>Reproduce una canción para ver su letra.</span>
          </div>
        ) : (
          reading
        )}
        {track && syncButton}
        {(error || trackError) && <p role="alert">{error || trackError}</p>}
        {loadedTrackId === trackId && snapshot.warnings.map((warning, index) => (
          <p key={index} role="status">{warning}</p>
        ))}
      </section>
    );
  return (
    <section
      ref={section}
      className={`resources lyrics-engine mode-${preferences.mode}${fullscreen ? " is-fullscreen" : ""}`}
      style={
        {
          "--lyric-size": `${preferences.size}px`,
          "--lyric-color": preferences.color,
          "--lyric-active": preferences.active_color,
          fontFamily:
            preferences.font === "system" ? "inherit" : preferences.font,
        } as CSSProperties
      }
    >
      {(preferences.mode === "minimal" ||
        preferences.mode === "fullscreen") && (
        <TrackArtwork
          className="lyric-backdrop"
          trackId={trackId}
          fallbackId={track?.artwork_id}
        />
      )}
      {!compact && selector}
      {!track ? (
        <p>
          Reproduce o selecciona una canción de la biblioteca para ver sus
          letras.
        </p>
      ) : (
        <>
          {!compact && (
            <div className="resource-heading">
              <div>
                <h2>{track.title}</h2>
                <p>{track.artist}</p>
              </div>
              <div className="button-group">
                <button
                  className="button"
                  onClick={() => audio.toggle()}
                  disabled={!audio.available}
                >
                  Reproducir / Pausar
                </button>
                {fullscreen && (
                  <button
                    className="button"
                    onClick={() => void document.exitFullscreen()}
                  >
                    Salir de pantalla completa
                  </button>
                )}
              </div>
            </div>
          )}
          <div className="resource-modes" aria-label="Modo de letras">
            {(
              [
                ["normal", "Normal"],
                ["karaoke", "Karaoke"],
                ["minimal", "Minimalista"],
                ["fullscreen", "Pantalla completa"],
              ] as const
            ).map(([id, title]) => (
              <button
                className="button"
                key={id}
                aria-pressed={preferences.mode === id}
                disabled={busy}
                onClick={() => void mode(id)}
              >
                {title}
              </button>
            ))}
          </div>
          {reading}
          {syncButton}
          {candidate && (
            <p className="secondary">
              {candidate.provider} ·{" "}
              {candidate.parsed.lines.some((l) => l.words.length)
                ? "Sincronización por palabra"
                : candidate.parsed.lines.some((l) => l.at !== null)
                  ? "Sincronización por línea"
                  : "Texto sin sincronización"}
              {candidate.source_url && (
                <>
                  {" "}
                  ·{" "}
                  <a
                    href={
                      candidate.source_url.startsWith("https://lrclib.net/")
                        ? candidate.source_url
                        : undefined
                    }
                    target="_blank"
                    rel="noreferrer"
                  >
                    Fuente
                  </a>
                </>
              )}
            </p>
          )}
          <details className="advanced-details lyric-tools">
            <summary>Opciones de letras</summary>
            <fieldset className="resource-controls" disabled={busy}>
              <legend>Presentación y sincronización</legend>
              <label>
                Desfase de esta canción (ms)
                <input
                  type="number"
                  min={-600000}
                  max={600000}
                  step={100}
                  value={snapshot.saved.offset}
                  onChange={(e) => settings({}, Number(e.target.value))}
                />
              </label>
              <label>
                Desfase global (ms)
                <input
                  type="number"
                  min={-600000}
                  max={600000}
                  step={100}
                  value={preferences.offset}
                  onChange={(e) => settings({ offset: Number(e.target.value) })}
                />
              </label>
              <label>
                Tamaño
                <select
                  value={preferences.size}
                  onChange={(e) => settings({ size: Number(e.target.value) })}
                >
                  {[20, 24, 30, 40, 56, 72].map((n) => (
                    <option key={n}>{n}</option>
                  ))}
                </select>
              </label>
              <label>
                Tipografía
                <select
                  value={preferences.font}
                  onChange={(e) => settings({ font: e.target.value })}
                >
                  <option value="system">Sistema</option>
                  <option value="serif">Serif</option>
                  <option value="sans-serif">Sans serif</option>
                </select>
              </label>
              <label>
                Texto
                <input
                  type="color"
                  value={preferences.color}
                  onChange={(e) => settings({ color: e.target.value })}
                />
              </label>
              <label>
                Línea activa
                <input
                  type="color"
                  value={preferences.active_color}
                  onChange={(e) => settings({ active_color: e.target.value })}
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={preferences.external}
                  onChange={(e) => settings({ external: e.target.checked })}
                />
                Permitir consultas a LRCLIB
              </label>
            </fieldset>
            <div className="resource-actions">
              <button
                className="button"
                disabled={busy}
                onClick={() => void importFile()}
              >
                Abrir LRC / TXT
              </button>
              <button
                className="button"
                disabled={busy}
                onClick={() => void importFile(true)}
              >
                Importar LyricsPlus v2
              </button>
              <button
                className="button"
                disabled={busy || !preferences.external}
                onClick={() => void run({ kind: "search", query: null })}
              >
                Buscar letras
              </button>
              <button
                className="button"
                disabled={busy}
                onClick={() => setEditor(candidate?.raw ?? "")}
              >
                Editar
              </button>
              <button
                className="button"
                disabled={busy || !snapshot.saved.selected}
                onClick={() => void exportFile()}
              >
                Descargar letra
              </button>
              <button
                className="button"
                disabled={busy || !candidate}
                onClick={() => void run({ kind: "delete" })}
              >
                Quitar letra guardada
              </button>
            </div>
            <form
              className="resource-actions"
              onSubmit={(e) => {
                e.preventDefault();
                const query = new FormData(e.currentTarget).get("query");
                void run({ kind: "search", query: String(query) });
              }}
            >
              <input
                name="query"
                aria-label="Buscar otra versión de letras"
                placeholder="Título, artista o versión"
                maxLength={500}
              />
              <button
                className="button"
                disabled={busy || !preferences.external}
              >
                Buscar otra versión
              </button>
            </form>
            {editor !== null && (
              <div className="lyric-editor">
                <label>
                  Texto LRC o TXT
                  <textarea
                    value={editor}
                    maxLength={1048576}
                    rows={9}
                    onChange={(e) => setEditor(e.target.value)}
                  />
                </label>
                <button
                  className="button"
                  disabled={busy}
                  onClick={() =>
                    void run({ kind: "save", raw: editor }).then(() =>
                      setEditor(null),
                    )
                  }
                >
                  Guardar edición local
                </button>
                <button className="button" onClick={() => setEditor(null)}>
                  Cancelar
                </button>
              </div>
            )}
            {!!snapshot.candidates.length && (
              <div>
                <h3>Elegir coincidencia</h3>
                <ul className="resource-results">
                  {snapshot.candidates.map((c) => (
                    <li key={c.id}>
                      <div>
                        <strong>
                          {c.title} · {c.artist}
                        </strong>
                        <p>
                          {c.album} · {c.duration ?? "?"} s · {c.provider} ·
                          coincidencia {c.score}/100
                        </p>
                        <p>
                          {c.parsed.lines
                            .slice(0, 2)
                            .map((l) => l.text)
                            .join(" / ")}
                        </p>
                      </div>
                      <button
                        className="button"
                        disabled={busy}
                        onClick={() =>
                          void run({ kind: "select", candidate: c })
                        }
                      >
                        Usar y guardar
                      </button>
                      <button
                        className="button"
                        disabled={busy}
                        onClick={() => void run({ kind: "reject", id: c.id })}
                      >
                        Rechazar
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <p className="inline-note">
              LyricsPlus: importación de respuestas v2; su servidor público
              documentado no está disponible. Las letras guardadas funcionan sin
              conexión.
            </p>
          </details>
        </>
      )}
      {(error || trackError) && <p role="alert">{error || trackError}</p>}
      {notice && <p role="status">{notice}</p>}
      {[...snapshot.warnings, ...(candidate?.parsed.warnings ?? [])].map(
        (w, i) => (
          <p key={i} role="status">
            {w}
          </p>
        ),
      )}
    </section>
  );
}
