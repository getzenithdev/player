import { useEffect, useRef, useState, type CSSProperties } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { AudioController } from "./audio";
import {
  activeLine,
  activeWord,
  defaultLyrics,
  lyricsAction,
  type LyricsPreferences,
  type LyricsSnapshot,
} from "./lyrics";
import { useResourceTrack } from "./ResourceTrack";
import "./Resources.css";
export function Lyrics({ audio }: { audio: AudioController }) {
  const { track, error: trackError, selector } = useResourceTrack(audio);
  const [snapshot, setSnapshot] = useState<LyricsSnapshot>(defaultLyrics);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [editor, setEditor] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const section = useRef<HTMLElement>(null);
  const active = useRef<HTMLParagraphElement>(null);
  const generation = useRef(0);
  const trackId = track?.id;
  useEffect(() => {
    const requests = generation;
    const token = ++requests.current;
    if (trackId)
      void lyricsAction(trackId, { kind: "snapshot" })
        .then((s) => {
          if (token === generation.current) {
            setSnapshot(s);
            setEditor(null);
            setError("");
            setBusy(false);
          }
        })
        .catch((e) => {
          if (token === generation.current) setError(String(e));
        });
    return () => {
      ++requests.current;
    };
  }, [trackId]);
  useEffect(() => {
    const change = () =>
      setFullscreen(document.fullscreenElement === section.current);
    document.addEventListener("fullscreenchange", change);
    return () => document.removeEventListener("fullscreenchange", change);
  }, []);
  const candidate =
    snapshot.saved.selected ??
    (!snapshot.saved.suppress_local
      ? snapshot.candidates.find(
          (c) => c.provider === "local" || c.provider === "embedded",
        )
      : null);
  const preferences = snapshot.preferences;
  const position =
    track &&
    (audio.state.track?.trackId === track.id ||
      audio.state.track?.path === track.local_path)
      ? audio.state.positionMs
      : 0;
  const offset =
    preferences.offset +
    snapshot.saved.offset +
    (candidate?.parsed.offset ?? 0);
  const line = activeLine(candidate?.parsed.lines ?? [], position, offset);
  useEffect(() => {
    active.current?.scrollIntoView({
      block: "center",
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches
        ? "instant"
        : "smooth",
    });
  }, [line]);
  async function run(action: object) {
    if (!trackId) return;
    const token = ++generation.current;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const s = await lyricsAction(trackId, action);
      if (token === generation.current) {
        setSnapshot(s);
        setNotice("Listo");
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
    void run({
      kind: "preferences",
      preferences: { ...preferences, ...patch },
      offset: trackOffset,
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
      {selector}
      {!track ? (
        <p>
          Reproduce o selecciona una canción de la biblioteca para ver sus
          letras.
        </p>
      ) : (
        <>
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
          <div className="lyric-lines" aria-label="Letra">
            {candidate ? (
              candidate.parsed.lines.map((l, i) => (
                <p
                  key={i}
                  ref={i === line ? active : undefined}
                  className={i === line ? "active" : ""}
                  aria-current={i === line ? "true" : undefined}
                >
                  {preferences.mode === "karaoke" && l.words.length
                    ? l.words.map((w, j) => (
                        <span
                          className={
                            j <= activeWord(l.words, position, offset)
                              ? "sung"
                              : ""
                          }
                          key={j}
                        >
                          {w.text}
                        </span>
                      ))
                    : l.text || "♪"}
                </p>
              ))
            ) : (
              <div className="resource-empty">
                No hay letras guardadas. Importa un archivo o busca
                coincidencias.
              </div>
            )}
          </div>
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
            <button className="button" disabled={busy || !preferences.external}>
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
                      onClick={() => void run({ kind: "select", candidate: c })}
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
