import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { pickAudioFiles, type AudioController } from "./audio";
import {
  emptyLibrary,
  groupKey,
  libraryAction,
  libraryViews,
  selectTracks,
  type LibraryAction,
  type LibraryTrack,
  type LibraryView,
  type MetadataEdit,
} from "./library";
import "./Library.css";
import { entryFromTrack, usePlaylists } from "./playlists";
import "./Playlists.css";

function Cover({ id }: { id: string | null }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    if (id && isTauri())
      void invoke<string>("library_artwork", { id })
        .then((value) => {
          if (!disposed) setUrl(value);
        })
        .catch(() => {});
    return () => {
      disposed = true;
    };
  }, [id]);
  return url ? (
    <img className="library-cover" src={url} alt="" />
  ) : (
    <span className="library-cover library-cover-empty" aria-hidden="true">
      ♫
    </span>
  );
}
export function Library({ audio }: { audio: AudioController }) {
  const lists = usePlaylists();
  const [library, setLibrary] = useState(emptyLibrary);
  const [view, setView] = useState<LibraryView>("Canciones");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<LibraryTrack | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const mounted = useRef(false);
  const generation = useRef(0);
  const native = isTauri();
  const reload = useCallback(async () => {
    const request = ++generation.current;
    try {
      const result = await libraryAction({ kind: "snapshot" });
      if (mounted.current && request === generation.current) setLibrary(result);
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    const cleanup: (() => void)[] = [];
    if (native)
      void (async () => {
        const changed = await listen("library:changed", () => {
          void reload();
        });
        if (disposed) changed();
        else cleanup.push(changed);
        const notices = await listen<string[]>("library:warnings", (e) =>
          setWarnings(e.payload),
        );
        if (disposed) notices();
        else cleanup.push(notices);
        if (!disposed) await reload();
      })().catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
      mounted.current = false;
      cleanup.forEach((f) => f());
    };
  }, [native, reload]);
  async function run(action: LibraryAction) {
    setBusy(true);
    setError(null);
    ++generation.current;
    try {
      const result = await libraryAction(action);
      if (mounted.current) {
        ++generation.current;
        setLibrary(result);
        setWarnings(result.warnings);
      }
      return true;
    } catch (reason) {
      if (mounted.current) setError(String(reason));
      return false;
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function add(folder: boolean) {
    setBusy(true);
    setError(null);
    try {
      const selected = folder
        ? await open({
            title: "Agregar carpetas de música",
            directory: true,
            multiple: true,
          })
        : await pickAudioFiles();
      const paths =
        selected === null
          ? []
          : Array.isArray(selected)
            ? selected
            : [selected];
      if (paths.length) await run({ kind: "import", paths });
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  const tracks = selectTracks(library, view, query);
  const groups = new Map<string, LibraryTrack[]>();
  for (const track of tracks) {
    const key = groupKey(track, view);
    const items = groups.get(key);
    if (items) items.push(track);
    else groups.set(key, [track]);
  }
  function play(selected: LibraryTrack) {
    const playable = tracks.filter((t) => !t.missing && t.local_path);
    const index = playable.findIndex((t) => t.id === selected.id);
    if (index < 0) return;
    // The selected song begins a queue in the current view's order.
    const ordered = [...playable.slice(index), ...playable.slice(0, index)];
    void audio.openFiles(
      ordered.map((t) => t.local_path!),
      true,
    );
  }
  return (
    <div className="music-library" aria-busy={busy}>
      <div className="toolbar">
        <label className="search-field">
          <span className="sr-only">Buscar en la biblioteca</span>
          <input
            type="search"
            placeholder="Buscar canciones, artistas o álbumes"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <button disabled={!native || busy} onClick={() => void add(false)}>
          Agregar archivos
        </button>
        <button disabled={!native || busy} onClick={() => void add(true)}>
          Agregar carpetas
        </button>
        <button
          disabled={!native || busy || !library.locations.length}
          onClick={() => void run({ kind: "rescan" })}
        >
          Volver a explorar
        </button>
      </div>
      {!native && (
        <p className="module-note">
          Abre la aplicación de escritorio para importar y reproducir tu música
          local.
        </p>
      )}
      {busy && <p role="status">Procesando biblioteca…</p>}
      {(error || lists.error) && (
        <p role="alert" className="library-error">
          {error || lists.error}
        </p>
      )}
      {!!warnings.length && (
        <details className="library-warnings">
          <summary>{warnings.length} avisos de lectura</summary>
          <ul>
            {warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}
      <div className="tabs library-tabs" aria-label="Vistas de biblioteca">
        {libraryViews.map((tab) => (
          <button
            key={tab}
            aria-pressed={view === tab}
            onClick={() => setView(tab)}
          >
            {tab}
          </button>
        ))}
      </div>
      <div className="list-heading">
        <span>{view}</span>
        <span>
          {tracks.length} {tracks.length === 1 ? "canción" : "canciones"}
        </span>
      </div>
      {view === "Duplicados" && (
        <p>
          Las coincidencias por SHA-256 se confirman por contenido completo. Las
          coincidencias por tamaño y metadatos son posibles duplicados. No se
          fusionan ni eliminan archivos.
        </p>
      )}
      {!tracks.length && (
        <div className="library-empty">
          <h2>{query ? "Sin resultados" : "Tu biblioteca empieza aquí"}</h2>
          <p>
            {query
              ? "Prueba otra búsqueda o vista."
              : "Agrega archivos o carpetas. La música permanece en su ubicación original."}
          </p>
        </div>
      )}
      {[...groups].map(([key, items]) => (
        <section key={key} aria-label={key || view}>
          {key && <h2 className="library-group">{key}</h2>}
          <div className="library-tracks">
            {items.map((t) => (
              <article className="library-track" key={t.id}>
                <Cover key={t.artwork_id} id={t.artwork_id} />
                <button
                  className="library-track-title"
                  disabled={t.missing}
                  onClick={() => play(t)}
                  title={t.local_path ?? undefined}
                >
                  <strong>{t.title}</strong>
                  <span>
                    {t.artist || "Artista desconocido"} ·{" "}
                    {t.album || "Sin álbum"}
                  </span>
                  <small>
                    {t.missing ? "Archivo no disponible" : t.format}
                    {t.play_count > 0
                      ? ` · ${t.play_count} reproducciones`
                      : ""}
                    {view === "Duplicados"
                      ? library.duplicates.some(
                          (g) => g.confirmed && g.ids.includes(t.id),
                        )
                        ? " · Contenido idéntico"
                        : " · Posible duplicado"
                      : ""}
                    {t.last_played && view === "Historial"
                      ? ` · ${new Date(t.last_played * 1000).toLocaleString()}`
                      : ""}
                  </small>
                </button>
                <div className="library-actions">
                  <button
                    className="button small"
                    disabled={t.missing || !t.local_path}
                    onClick={() =>
                      void audio.queueAction({
                        kind: "append",
                        paths: [t.local_path!],
                        after_current: true,
                      })
                    }
                  >
                    Reproducir después
                  </button>
                  <button
                    className="button small"
                    disabled={t.missing || !t.local_path}
                    onClick={() =>
                      void audio.queueAction({
                        kind: "append",
                        paths: [t.local_path!],
                        after_current: false,
                      })
                    }
                  >
                    Añadir al final
                  </button>
                  <label>
                    <span className="sr-only">
                      Añadir {t.title} a una lista
                    </span>
                    <select
                      value=""
                      disabled={lists.busy || !lists.playlists.length}
                      onChange={(e) => {
                        if (e.target.value)
                          void lists.run({
                            kind: "add",
                            id: e.target.value,
                            entries: [entryFromTrack(t)],
                          });
                      }}
                    >
                      <option value="">Añadir a lista…</option>
                      {lists.playlists.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <span className="library-duration">
                  {t.duration === null
                    ? "—"
                    : `${Math.floor(t.duration / 60)}:${String(t.duration % 60).padStart(2, "0")}`}
                </span>
                <button
                  disabled={busy}
                  aria-label={`${t.favorite ? "Quitar de" : "Agregar a"} favoritos: ${t.title}`}
                  aria-pressed={t.favorite}
                  onClick={() =>
                    void run({
                      kind: "favorite",
                      id: t.id,
                      favorite: !t.favorite,
                    })
                  }
                >
                  {t.favorite ? "♥" : "♡"}
                </button>
                <button disabled={busy} onClick={() => setEditing(t)}>
                  Editar
                </button>
              </article>
            ))}
          </div>
        </section>
      ))}
      <details className="library-locations">
        <summary>Ubicaciones ({library.locations.length})</summary>
        <p>
          Se revisan cada 10 segundos mientras la aplicación está abierta.
          Quitar una ubicación solo retira las pistas que no cubra otra
          ubicación.
        </p>
        {library.locations.map((path) => (
          <div key={path}>
            <code>{path}</code>
            <button
              disabled={busy}
              onClick={() => void run({ kind: "removeLocation", path })}
            >
              Quitar ubicación
            </button>
          </div>
        ))}
      </details>
      {editing && (
        <MetadataEditor
          key={editing.id}
          track={editing}
          busy={busy}
          error={error}
          close={() => setEditing(null)}
          save={async (metadata, write_tags) => {
            if (
              await run({ kind: "edit", id: editing.id, metadata, write_tags })
            )
              setEditing(null);
          }}
        />
      )}
    </div>
  );
}
function MetadataEditor({
  track,
  busy,
  error,
  close,
  save,
}: {
  track: LibraryTrack;
  busy: boolean;
  error: string | null;
  close: () => void;
  save: (metadata: MetadataEdit, writeTags: boolean) => Promise<void>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [writeTags, setWriteTags] = useState(false);
  useEffect(() => {
    const el = dialog.current;
    el?.showModal();
    return () => el?.close();
  }, []);
  const textFields = [
    ["title", "Título"],
    ["artist", "Artista"],
    ["album_artist", "Artista del álbum"],
    ["album", "Álbum"],
    ["genre", "Género"],
    ["comments", "Comentarios"],
  ] as const;
  const numberFields = [
    ["year", "Año"],
    ["track_number", "Número de pista"],
    ["disc_number", "Número de disco"],
  ] as const;
  return (
    <dialog
      ref={dialog}
      className="metadata-dialog"
      aria-labelledby="metadata-title"
      onCancel={(event) => {
        if (busy) event.preventDefault();
        else close();
      }}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const data = new FormData(e.currentTarget);
          const metadata: MetadataEdit = {
            title: "",
            artist: null,
            album_artist: null,
            album: null,
            genre: null,
            comments: null,
            year: null,
            track_number: null,
            disc_number: null,
          };
          for (const [key] of textFields) {
            const value = String(data.get(key) ?? "").trim();
            if (key === "title") metadata.title = value;
            else metadata[key] = value || null;
          }
          for (const [key] of numberFields) {
            const value = String(data.get(key) ?? "");
            metadata[key] = value ? Number(value) : null;
          }
          void save(metadata, writeTags);
        }}
      >
        <h2 id="metadata-title">Editar metadatos</h2>
        {error && (
          <p role="alert" className="library-error">
            {error}
          </p>
        )}
        <fieldset disabled={busy}>
          {textFields.map(([key, label]) => (
            <label key={key}>
              {label}
              <input
                name={key}
                required={key === "title"}
                maxLength={16384}
                defaultValue={track[key] ?? ""}
              />
            </label>
          ))}
          <div className="metadata-numbers">
            {numberFields.map(([key, label]) => (
              <label key={key}>
                {label}
                <input
                  type="number"
                  name={key}
                  min="0"
                  max={key === "year" ? "65535" : "4294967295"}
                  step="1"
                  defaultValue={track[key] ?? ""}
                />
              </label>
            ))}
          </div>
          <label className="metadata-write">
            <input
              type="checkbox"
              checked={writeTags}
              disabled={track.missing}
              onChange={(e) => setWriteTags(e.target.checked)}
            />
            Escribir también las etiquetas en el archivo original
          </label>
          <p>
            {writeTags
              ? "Se modificará el archivo original. Se conservará una copia .zenith-….bak junto al archivo para recuperarlo."
              : "Los cambios se guardan solo en Zenith. El archivo original permanece intacto."}
          </p>
          <footer>
            <button type="button" onClick={close}>
              Cancelar
            </button>
            <button type="submit">
              {writeTags
                ? "Guardar y escribir archivo"
                : "Guardar en biblioteca"}
            </button>
          </footer>
        </fieldset>
      </form>
    </dialog>
  );
}
