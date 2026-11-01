import { useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { AudioController } from "./audio";
import {
  artworkAction,
  emptyArtwork,
  safeSourceUrl,
  type ArtworkPreferences,
} from "./artwork";
import { TrackArtwork } from "./TrackArtwork";
import { useResourceTrack } from "./ResourceTrack";
import "./Resources.css";
export function Artwork({ audio }: { audio: AudioController }) {
  const { track, selector, error: trackError } = useResourceTrack(audio);
  const [snapshot, setSnapshot] = useState(emptyArtwork);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [paused, setPaused] = useState(false);
  const requests = useRef(0);
  const trackId = track?.id;
  useEffect(() => {
    const ref = requests;
    const token = ++ref.current;
    if (trackId)
      void artworkAction(trackId, { kind: "snapshot" })
        .then((s) => {
          if (token === ref.current) {
            setSnapshot(s);
            setBusy(false);
            setError("");
          }
        })
        .catch((e) => {
          if (token === ref.current) setError(String(e));
        });
    return () => {
      ++ref.current;
    };
  }, [trackId]);
  async function run(action: object) {
    if (!trackId) return;
    const token = ++requests.current;
    setBusy(true);
    setError("");
    try {
      const s = await artworkAction(trackId, action);
      if (token === requests.current) setSnapshot(s);
    } catch (e) {
      if (token === requests.current) setError(String(e));
    } finally {
      if (token === requests.current) setBusy(false);
    }
  }
  async function importFile(manifest = false) {
    try {
      const path = await open({
        multiple: false,
        filters: [
          {
            name: manifest ? "Manifiesto autorizado" : "Portada",
            extensions: manifest
              ? ["json"]
              : ["png", "jpg", "jpeg", "webp", "gif"],
          },
        ],
      });
      if (typeof path === "string")
        await run({ kind: manifest ? "importManifest" : "import", path });
    } catch (e) {
      setError(String(e));
    }
  }
  const p = snapshot.preferences;
  function settings(patch: Partial<ArtworkPreferences>) {
    void run({ kind: "preferences", preferences: { ...p, ...patch } });
  }
  async function exportFile() {
    if (!snapshot.resolved) return;
    try {
      const ext = snapshot.resolved.id.split(".").pop();
      const path = await save({ defaultPath: `cover.${ext}` });
      if (path) await run({ kind: "export", path, id: snapshot.resolved.id });
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <section className="resources">
      {selector}
      {!track ? (
        <p>
          Reproduce o selecciona una canción de la biblioteca para gestionar sus
          portadas.
        </p>
      ) : (
        <>
          <div className="artwork-preview">
            <TrackArtwork
              key={track.id}
              trackId={track.id}
              fallbackId={track.artwork_id}
              animate
              paused={paused}
            />
            <div>
              <h2>{track.title}</h2>
              <p>
                {track.artist} · {track.album}
              </p>
              {snapshot.resolved ? (
                <div className="resource-attribution">
                  <p>
                    {snapshot.resolved.provider} · {snapshot.resolved.width} ×{" "}
                    {snapshot.resolved.height} ·{" "}
                    {snapshot.resolved.animated ? "Animada" : "Estática"}
                  </p>
                  <p>{snapshot.resolved.attribution}</p>
                  <p>{snapshot.resolved.license}</p>
                  {safeSourceUrl(snapshot.resolved.source_url) && (
                    <a
                      href={safeSourceUrl(snapshot.resolved.source_url)}
                      target="_blank"
                      rel="noreferrer"
                    >
                      Ver fuente y derechos
                    </a>
                  )}
                </div>
              ) : (
                <p>Se usará la portada local o la imagen predeterminada.</p>
              )}
              <p className="inline-note">
                Prioridad: Apple Music animada → Apple Music estática →
                Archive.org → YouTube → local → predeterminada. La selección
                manual tiene preferencia.
              </p>
              <button
                className="button"
                onClick={() => setPaused((v) => !v)}
                disabled={!snapshot.resolved?.animated}
              >
                {paused ? "Reanudar animación" : "Pausar animación"}
              </button>
            </div>
          </div>
          <fieldset className="resource-controls" disabled={busy}>
            <legend>Animación y consultas</legend>
            <label>
              <input
                type="checkbox"
                checked={p.external}
                onChange={(e) => settings({ external: e.target.checked })}
              />
              Permitir consultas a Archive.org
            </label>
            <label>
              <input
                type="checkbox"
                checked={p.autoplay}
                onChange={(e) => settings({ autoplay: e.target.checked })}
              />
              Reproducción automática
            </label>
            <label>
              <input
                type="checkbox"
                checked={p.looping}
                onChange={(e) => settings({ looping: e.target.checked })}
              />
              Repetir animación
            </label>
            <label>
              <input
                type="checkbox"
                checked={p.battery_saver}
                onChange={(e) => settings({ battery_saver: e.target.checked })}
              />
              Ahorro de batería (imagen estática)
            </label>
            <label>
              Calidad
              <select
                value={p.quality}
                onChange={(e) =>
                  settings({
                    quality: e.target.value as ArtworkPreferences["quality"],
                  })
                }
              >
                <option value="thumbnail">Miniatura</option>
                <option value="preview">Vista previa</option>
              </select>
            </label>
          </fieldset>
          <div className="resource-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => void importFile()}
            >
              Abrir imagen / GIF
            </button>
            <button
              className="button"
              disabled={busy}
              onClick={() => void importFile(true)}
            >
              Importar recurso autorizado
            </button>
            <button
              className="button"
              disabled={busy}
              onClick={() => void run({ kind: "local" })}
            >
              Buscar archivos locales
            </button>
            <button
              className="button"
              disabled={busy || !p.external}
              onClick={() => void run({ kind: "search", query: null })}
            >
              Buscar portada
            </button>
            <button
              className="button"
              disabled={busy || !snapshot.resolved}
              onClick={() => void exportFile()}
            >
              Guardar original como…
            </button>
            <button
              className="button"
              disabled={busy || !snapshot.candidates.length}
              onClick={() => void run({ kind: "remove" })}
            >
              Restablecer portadas
            </button>
          </div>
          <form
            className="resource-actions"
            onSubmit={(e) => {
              e.preventDefault();
              const query = String(new FormData(e.currentTarget).get("query"));
              void run({ kind: "search", query });
            }}
          >
            <input
              name="query"
              aria-label="Búsqueda de portada por artista álbum o identificador"
              placeholder="Artista, álbum, título, año o identificador"
              maxLength={500}
            />
            <button className="button" disabled={busy || !p.external}>
              Buscar otra coincidencia
            </button>
          </form>
          <p className="inline-note">
            Apple Music y YouTube aceptan recursos obtenidos con autorización
            mediante un manifiesto local. La consulta directa requiere conectar
            sus APIs. Archive.org permite revisar candidatos y descargar
            imágenes con licencia Creative Commons reconocida. La animación
            admite GIF hasta 120 cuadros y se detiene fuera de vista, al ocultar
            la ventana y con movimiento reducido.
          </p>
          {!!snapshot.candidates.length && (
            <>
              <h3>Revisar portadas</h3>
              <ul className="resource-results">
                {snapshot.candidates.map((c, i) => (
                  <li key={`${c.id}-${i}`}>
                    <div>
                      <strong>
                        {c.title || "Portada"} · {c.artist}
                      </strong>
                      <p>
                        {c.provider} · coincidencia {c.score}/100 ·{" "}
                        {c.license || "Revisar licencia"}
                      </p>
                      {safeSourceUrl(c.source_url) && (
                        <a
                          href={safeSourceUrl(c.source_url)}
                          target="_blank"
                          rel="noreferrer"
                        >
                          Fuente
                        </a>
                      )}
                    </div>
                    <button
                      className="button"
                      disabled={busy}
                      onClick={() =>
                        void run(
                          c.id.startsWith("archive:")
                            ? { kind: "archive", identifier: c.id.slice(8) }
                            : { kind: "select", id: c.id },
                        )
                      }
                    >
                      {c.id.startsWith("archive:")
                        ? "Validar y guardar"
                        : "Usar portada"}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
      {(error || trackError) && <p role="alert">{error || trackError}</p>}
      {snapshot.warnings.map((w, i) => (
        <p role="status" key={i}>
          {w}
        </p>
      ))}
      {busy && <p role="status">Procesando portada…</p>}
    </section>
  );
}
