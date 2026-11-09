import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { pickAudioFiles, type AudioController } from "./audio";
import {
  emptyLibrary,
  groupKey,
  groupIdentity,
  metadataKey,
  libraryAction,
  libraryViews,
  selectTracks,
  type LibraryAction,
  type LibraryTrack,
  type LibraryView,
  type MetadataEdit,
} from "./library";
import "./Library.css";
import { TrackArtwork } from "./TrackArtwork";
import { Icon } from "./Icon";
import { entryFromTrack, usePlaylists } from "./playlists";
import "./Playlists.css";
import { ArtistArtwork, forgetArtistPhotos } from "./ArtistArtwork";
import { CollectionDetail } from "./CollectionDetail";
import { albumGroups } from "./musicViews";
import type { CollectionRoute, LibraryLocation } from "./navigation";

export function Library({
  audio,
  initialView = "Canciones",
  initialQuery = "",
  collection,
  onOpenCollection,
  onBack,
  onReady,
  backgroundTarget,
  onCollectionDeleted,
}: {
  audio: AudioController;
  initialView?: LibraryView;
  initialQuery?: string;
  collection?: CollectionRoute;
  onOpenCollection?: (
    collection: CollectionRoute,
    location?: LibraryLocation,
  ) => void;
  onBack?: () => void;
  onReady?: () => void;
  backgroundTarget?: HTMLElement | null;
  onCollectionDeleted?: () => void;
}) {
  const lists = usePlaylists();
  const [library, setLibrary] = useState(emptyLibrary);
  useLayoutEffect(() => {
    if (library !== emptyLibrary) onReady?.();
  }, [library, onReady]);
  const [view, setView] = useState<LibraryView>(
    collection
      ? collection.kind === "album"
        ? "Álbumes"
        : "Artistas"
      : initialView,
  );
  const [query, setQuery] = useState(initialQuery);
  const [selectedGroup, setSelectedGroup] = useState<string | null>(
    collection?.key ?? null,
  );
  const [selectedArtistAlbum, setSelectedArtistAlbum] = useState<string | null>(
    null,
  );
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
  const tracks = useMemo(
    () => selectTracks(library, view, query),
    [library, view, query],
  );
  const groups = useMemo(() => {
    const result = new Map<string, LibraryTrack[]>();
    for (const track of tracks) {
      const key = groupIdentity(track, view);
      const items = result.get(key);
      if (items) items.push(track);
      else result.set(key, [track]);
    }
    return result;
  }, [tracks, view]);
  const albums = useMemo(
    () =>
      albumGroups(library.tracks).map((items) =>
        items.sort(
          (a, b) =>
            (a.disc_number ?? 0) - (b.disc_number ?? 0) ||
            (a.track_number ?? 0) - (b.track_number ?? 0) ||
            a.title.localeCompare(b.title),
        ),
      ),
    [library.tracks],
  );
  const selectedArtist = useMemo(() => {
    if (view !== "Artistas" || selectedGroup === null) return undefined;
    const exact = library.tracks.find(
      (track) => groupIdentity(track, "Artistas") === selectedGroup,
    );
    if (exact) return exact;
    const albumArtist = library.tracks.find(
      (track) =>
        track.album_artist && metadataKey(track.album_artist) === selectedGroup,
    );
    return albumArtist
      ? {
          ...albumArtist,
          artist: albumArtist.album_artist,
          metadata: undefined,
        }
      : undefined;
  }, [library.tracks, view, selectedGroup]);
  const artistAlbums = useMemo(() => {
    if (!selectedArtist) return [];
    const name = metadataKey(selectedArtist.artist ?? "");
    return albums
      .filter((items) =>
        items.some(
          (track) =>
            groupIdentity(track, "Artistas") === selectedGroup ||
            (name &&
              metadataKey(track.album_artist ?? "") === name &&
              metadataKey(track.artist ?? "") !== name),
        ),
      )
      .sort((a, b) => (b[0].year ?? 0) - (a[0].year ?? 0));
  }, [albums, selectedArtist, selectedGroup]);
  const detailTracks = useMemo(() => {
    if (selectedGroup === null) return [];
    if (view === "Álbumes")
      return (
        albums.find(
          (items) => groupIdentity(items[0], "Álbumes") === selectedGroup,
        ) ?? []
      );
    if (view === "Artistas") {
      if (selectedArtistAlbum)
        return (
          artistAlbums.find(
            (items) =>
              groupIdentity(items[0], "Álbumes") === selectedArtistAlbum,
          ) ?? []
        );
      return library.tracks.filter(
        (track) =>
          groupIdentity(track, "Artistas") === selectedGroup ||
          (!!selectedArtist?.artist &&
            metadataKey(track.album_artist ?? "") ===
              metadataKey(selectedArtist.artist) &&
            metadataKey(track.artist ?? "") !==
              metadataKey(selectedArtist.artist)),
      );
    }
    return [];
  }, [
    selectedGroup,
    selectedArtistAlbum,
    view,
    albums,
    artistAlbums,
    library.tracks,
    selectedArtist,
  ]);
  const detailArtist = useMemo(() => {
    const first = detailTracks[0];
    if (!first) return undefined;
    if (view === "Artistas" && !selectedArtistAlbum) return selectedArtist;
    const name = first.album_artist ?? first.artist;
    return (
      library.tracks.find(
        (track) => metadataKey(track.artist ?? "") === metadataKey(name ?? ""),
      ) ?? { ...first, artist: name, metadata: undefined }
    );
  }, [detailTracks, library.tracks, selectedArtist, selectedArtistAlbum, view]);
  const visibleGroups = detailTracks.length
    ? [[selectedGroup!, detailTracks] as const]
    : [...groups].filter(
        ([key]) => selectedGroup === null || selectedGroup === key,
      );
  function play(selected: LibraryTrack) {
    const playable = (detailTracks.length ? detailTracks : tracks).filter(
      (t) =>
        !t.missing &&
        t.local_path &&
        (detailTracks.length > 0 ||
          selectedGroup === null ||
          groupIdentity(t, view) === selectedGroup),
    );
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
    <div
      className={`music-library${collection ? " collection-page" : ""}`}
      aria-busy={busy}
    >
      {!collection && (
        <>
          <div className="toolbar">
            <label className="search-field">
              <span className="sr-only">Buscar en la biblioteca</span>
              <input
                type="search"
                placeholder="Buscar canciones, artistas o álbumes"
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setSelectedGroup(null);
                  setSelectedArtistAlbum(null);
                }}
              />
            </label>
            <button disabled={!native || busy} onClick={() => void add(false)}>
              Agregar archivos
            </button>
            <button disabled={!native || busy} onClick={() => void add(true)}>
              Agregar carpetas
            </button>
            <details className="library-tools advanced-details">
              <summary>Organizar biblioteca</summary>
              <div className="resource-actions">
                <button
                  disabled={!native || busy}
                  onClick={() =>
                    void (async () => {
                      setBusy(true);
                      setError(null);
                      try {
                        const path = await open({
                          title: "Importar metadatos de biblioteca",
                          multiple: false,
                          directory: false,
                          filters: [
                            { name: "Metadatos Zenith", extensions: ["json"] },
                          ],
                        });
                        if (typeof path === "string")
                          await run({ kind: "importMetadata", path });
                      } catch (reason) {
                        setError(String(reason));
                      } finally {
                        setBusy(false);
                      }
                    })()
                  }
                >
                  Importar metadatos JSON
                </button>
                <button
                  disabled={!native || busy || !library.locations.length}
                  onClick={() => void run({ kind: "rescan" })}
                >
                  Volver a explorar
                </button>
              </div>
            </details>
          </div>
          {!native && (
            <p className="module-note">
              Importa música desde la aplicación de escritorio.
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
          {library.metadata_analysis &&
            (library.metadata_analysis.album_candidates.length > 0 ||
              library.metadata_analysis.artist_collisions.length > 0) && (
              <details className="library-warnings">
                <summary>Coincidencias e identidades para revisar</summary>
                <p>
                  Las entradas se conservan separadas. Una coincidencia de
                  nombre no confirma que sea la misma edición o el mismo
                  artista.
                </p>
                <ul>
                  {library.metadata_analysis.album_candidates.map((a, i) => (
                    <li key={"album-" + i}>
                      {a.name}: {a.album_keys.length} grupos de edición o
                      identidad.
                    </li>
                  ))}
                  {library.metadata_analysis.artist_collisions.map((a, i) => (
                    <li key={"artist-" + i}>
                      {a.name}: {a.identities.length} identidades de artista.
                    </li>
                  ))}
                </ul>
              </details>
            )}
          <div className="library-viewbar">
            <label>
              <span className="sr-only">Vista de biblioteca</span>
              <select
                aria-label="Vista de biblioteca"
                value={view}
                onChange={(event) => {
                  setView(event.target.value as LibraryView);
                  setSelectedGroup(null);
                  setSelectedArtistAlbum(null);
                }}
              >
                {libraryViews.map((tab) => (
                  <option key={tab} value={tab}>
                    {tab}
                  </option>
                ))}
              </select>
            </label>
            <span>
              {tracks.length} {tracks.length === 1 ? "canción" : "canciones"}
            </span>
          </div>
          {view === "Duplicados" && (
            <p>
              Las coincidencias por SHA-256 se confirman por contenido completo.
              Las coincidencias por tamaño y metadatos son posibles duplicados.
              No se fusionan ni eliminan archivos.
            </p>
          )}
          {!tracks.length && (
            <div className="library-empty">
              <h2>{query ? "Sin resultados" : "Biblioteca vacía"}</h2>
              <p>
                {query
                  ? "Prueba otra búsqueda o vista."
                  : "Agrega archivos o una carpeta."}
              </p>
            </div>
          )}
        </>
      )}
      {(collection || selectedGroup !== null) && (
        <button
          className="text-button collection-back"
          onClick={() => {
            if (collection) {
              onBack?.();
              return;
            }
            if (selectedArtistAlbum) setSelectedArtistAlbum(null);
            else setSelectedGroup(null);
          }}
        >
          <span aria-hidden="true">←</span>{" "}
          {collection
            ? "Volver"
            : selectedArtistAlbum
              ? `Volver a ${selectedArtist?.artist ?? "artista"}`
              : `Volver a ${view.toLocaleLowerCase()}`}
        </button>
      )}
      <div
        className={
          detailTracks.length > 0 ? "collection-presentation" : undefined
        }
      >
        {detailTracks.length > 0 && detailArtist && (
          <CollectionDetail
            onDelete={async (expectedIds) => {
              const album = view === "Álbumes" || !!selectedArtistAlbum;
              const key = selectedArtistAlbum ?? selectedGroup;
              if (!key)
                throw new Error("Selecciona una colección para eliminar");
              ++generation.current;
              setBusy(true);
              try {
                const result = await libraryAction({
                  kind: "removeCollection",
                  collection: album ? "album" : "artist",
                  key,
                  expected_ids: expectedIds,
                });
                if (mounted.current) {
                  ++generation.current;
                  setLibrary(result);
                  setWarnings(result.warnings);
                  forgetArtistPhotos();
                  if (onCollectionDeleted) onCollectionDeleted();
                  else {
                    setSelectedGroup(null);
                    setSelectedArtistAlbum(null);
                  }
                }
              } finally {
                if (mounted.current) setBusy(false);
              }
            }}
            backgroundTarget={backgroundTarget}
            key={`${view}-${selectedGroup}-${selectedArtistAlbum ?? ""}`}
            kind={
              view === "Artistas" && !selectedArtistAlbum ? "artist" : "album"
            }
            tracks={detailTracks}
            artist={detailArtist}
            onArtist={() => {
              if (onOpenCollection) {
                onOpenCollection({
                  kind: "artist",
                  key: groupIdentity(detailArtist, "Artistas"),
                  title: detailArtist.artist ?? "Artista desconocido",
                });
                return;
              }
              setView("Artistas");
              setQuery("");
              setSelectedGroup(groupIdentity(detailArtist, "Artistas"));
              setSelectedArtistAlbum(null);
            }}
            onPlay={() => {
              const first = detailTracks.find(
                (track) => track.local_path && !track.missing,
              );
              if (first) play(first);
            }}
          />
        )}
        {selectedArtist && !selectedArtistAlbum && artistAlbums.length > 0 && (
          <section
            className="artist-discography"
            aria-label={`Álbumes de ${selectedArtist.artist ?? "artista"}`}
          >
            <div className="section-heading">
              <h2>Álbumes</h2>
              <span className="collection-subtitle">{artistAlbums.length}</span>
            </div>
            <div className="library-gallery">
              {artistAlbums.map((items) => (
                <button
                  className="collection-card"
                  key={groupIdentity(items[0], "Álbumes")}
                  onClick={() => {
                    if (onOpenCollection)
                      onOpenCollection({
                        kind: "album",
                        key: groupIdentity(items[0], "Álbumes"),
                        title: items[0].album ?? "Sin álbum",
                      });
                    else
                      setSelectedArtistAlbum(
                        groupIdentity(items[0], "Álbumes"),
                      );
                  }}
                >
                  <TrackArtwork
                    className="collection-art"
                    trackId={items[0].id}
                    fallbackId={items[0].artwork_id}
                  />
                  <strong>{items[0].album ?? "Sin álbum"}</strong>
                  <span>
                    {[items[0].year, `${items.length} canciones`]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </button>
              ))}
            </div>
          </section>
        )}
        {selectedGroup === null &&
        ["Álbumes", "Artistas", "Géneros"].includes(view) ? (
          <div
            className={`library-gallery ${view === "Artistas" ? "artist-gallery" : ""}`}
          >
            {[...groups].map(([key, items]) => (
              <button
                className="collection-card"
                key={key}
                onClick={() => {
                  if (
                    onOpenCollection &&
                    (view === "Álbumes" || view === "Artistas")
                  ) {
                    onOpenCollection(
                      {
                        kind: view === "Álbumes" ? "album" : "artist",
                        key,
                        title:
                          view === "Álbumes"
                            ? (items[0].album ?? "Sin álbum")
                            : (items[0].artist ?? "Artista desconocido"),
                      },
                      { view, query },
                    );
                    return;
                  }
                  setSelectedGroup(key);
                  setSelectedArtistAlbum(null);
                }}
              >
                {view === "Artistas" ? (
                  <ArtistArtwork
                    className="collection-art"
                    artistKey={key}
                    name={items[0].artist ?? "Artista desconocido"}
                    trackId={items[0].id}
                  />
                ) : (
                  <TrackArtwork
                    className="collection-art"
                    trackId={items[0].id}
                    fallbackId={items[0].artwork_id}
                  />
                )}
                <strong>{groupKey(items[0], view)}</strong>
                <span>
                  {items.length} {items.length === 1 ? "canción" : "canciones"}
                </span>
              </button>
            ))}
          </div>
        ) : (
          visibleGroups.map(([key, items]) => (
            <section key={key} aria-label={groupKey(items[0], view) || view}>
              {(detailTracks.length > 0 || groupKey(items[0], view)) && (
                <h2 className="library-group">
                  {detailTracks.length > 0
                    ? "Canciones"
                    : groupKey(items[0], view)}
                </h2>
              )}
              <div className="library-tracks">
                {items.map((t) => (
                  <article className="library-track" key={t.id}>
                    <TrackArtwork
                      className="library-cover"
                      trackId={t.id}
                      fallbackId={t.artwork_id}
                    />
                    <button
                      className="library-track-title"
                      disabled={t.missing || !t.local_path}
                      onClick={() => play(t)}
                      title={t.local_path ?? undefined}
                    >
                      <strong>{t.title || "Sin título"}</strong>
                      <span>
                        {t.artist || "Artista desconocido"} ·{" "}
                        {t.album || "Sin álbum"}
                      </span>
                      <small>
                        {!t.local_path
                          ? "Sin archivo local"
                          : t.missing
                            ? "Archivo no disponible"
                            : t.format}
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
                    <details className="track-menu">
                      <summary aria-label={`Más opciones: ${t.title}`}>
                        <Icon name="more" />
                      </summary>
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
                        <button disabled={busy} onClick={() => setEditing(t)}>
                          Editar información
                        </button>
                      </div>
                    </details>
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
                      <Icon name="heart" size={18} />
                    </button>
                  </article>
                ))}
              </div>
            </section>
          ))
        )}
      </div>
      {collection && (error || lists.error) && (
        <p className="library-error" role="alert">
          {error || lists.error}
        </p>
      )}
      {collection && !detailTracks.length && !error && (
        <p className="inline-note" role="status">
          {library !== emptyLibrary
            ? "Esta colección ya no está en tu biblioteca."
            : "Cargando colección…"}
        </p>
      )}
      {!collection && (
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
      )}
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
        {track.metadata && (
          <details className="metadata-provenance">
            <summary>Origen, valores originales e identificadores</summary>
            <p>
              Fuente: {track.source || "Desconocida"}
              {track.source_id ? ` · ${track.source_id}` : ""}
            </p>
            <pre>
              {JSON.stringify(
                {
                  original: track.metadata.original,
                  identifiers: track.metadata.identifiers,
                  edition: track.metadata.edition,
                  track_version: track.metadata.track_version,
                  remixer: track.metadata.remixer,
                },
                null,
                2,
              )}
            </pre>
          </details>
        )}

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
