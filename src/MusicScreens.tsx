import { useEffect, useState } from "react";
import type { AudioController } from "./audio";
import { Icon } from "./Icon";
import { TrackArtwork } from "./TrackArtwork";
import { ExternalSearchResults } from "./ExternalSearchResults";
import { groupIdentity, groupKey, type LibraryTrack } from "./library";
import {
  albumGroups,
  formatTime,
  playableTracks,
  searchMusic,
} from "./musicViews";
import { entryFromPath, entryFromTrack, usePlaylists } from "./playlists";
import type { AppView } from "./navigation";
import {
  searchExternalSources,
  sourcesAction,
  type SourceEntry,
} from "./sources";

type ScreenProps = {
  audio: AudioController;
  tracks: LibraryTrack[];
  navigate: (view: AppView) => void;
};

function AlbumShelf({
  title,
  groups,
  audio,
  onBrowse,
}: {
  title: string;
  groups: LibraryTrack[][];
  audio: AudioController;
  onBrowse: () => void;
}) {
  if (!groups.length) return null;
  return (
    <section className="music-section">
      <div className="section-heading">
        <h2>{title}</h2>
        <button className="text-button" onClick={onBrowse}>
          Ver todos
        </button>
      </div>
      <div className="album-shelf">
        {groups.slice(0, 5).map((items) => (
          <button
            className="album-card"
            key={items[0].id}
            disabled={!audio.available || !playableTracks(items).length}
            onClick={() =>
              void audio.openEntries(playableTracks(items).map(entryFromTrack))
            }
          >
            <span className="album-image">
              <TrackArtwork
                className="album-art"
                trackId={items[0].id}
                fallbackId={items[0].artwork_id}
              />
              <span className="album-play">
                <Icon name="play" />
              </span>
            </span>
            <strong>{items[0].album ?? "Sin álbum"}</strong>
            <span>
              {items[0].album_artist ??
                items[0].artist ??
                "Artista desconocido"}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}

export function Home({
  audio,
  tracks,
  navigate,
  onImport,
}: ScreenProps & { onImport: () => void }) {
  const current = tracks.find(
    (t) =>
      t.id === audio.state.track?.trackId ||
      t.local_path === audio.state.track?.path,
  );
  const recent = [...tracks]
    .sort((a, b) => (b.last_played ?? 0) - (a.last_played ?? 0))
    .filter((t) => t.last_played);
  const added = [...tracks].sort((a, b) => b.added_at - a.added_at);
  const favorites = tracks.filter((t) => t.favorite);
  const mostPlayed = [...tracks]
    .filter((t) => t.play_count > 0)
    .sort((a, b) => b.play_count - a.play_count);
  const featured = current ?? recent[0] ?? added[0];
  return (
    <div className="home-screen">
      <section className="listening-hero">
        <div className={`hero-cover ${featured ? "" : "brand-cover"}`}>
          {featured ? (
            <TrackArtwork
              className="hero-art"
              trackId={featured.id}
              fallbackId={featured.artwork_id}
              animate
              paused={audio.state.status !== "playing"}
            />
          ) : (
            <>
              <img src="/favicon.svg" alt="" />
            </>
          )}
        </div>
        <div className="hero-copy">
          {(current || recent.length > 0) && (
            <p className="hero-kicker">
              {current ? "Ahora suena" : recent.length ? "Reciente" : ""}
            </p>
          )}
          <h2>{featured?.title ?? "Tu música."}</h2>
          <p className="hero-description">
            {featured
              ? `${featured.artist ?? "Artista desconocido"}${featured.album ? ` · ${featured.album}` : ""}`
              : "Agrega archivos o conecta una fuente."}
          </p>
          <div className="hero-actions">
            <button
              className="button primary"
              disabled={!audio.available}
              onClick={() => {
                if (current) void audio.toggle();
                else if (featured && playableTracks([featured]).length)
                  void audio.openEntries([entryFromTrack(featured)]);
                else onImport();
              }}
            >
              <Icon
                name={
                  current && audio.state.status === "playing"
                    ? "pause"
                    : featured
                      ? "play"
                      : "plus"
                }
                size={19}
              />
              {current && audio.state.status === "playing"
                ? "Pausar"
                : featured
                  ? "Escuchar ahora"
                  : "Agregar música"}
            </button>
            <button
              className="button subtle"
              onClick={() => navigate(featured ? "nowPlaying" : "sources")}
            >
              {featured ? "Abrir reproductor" : "Conectar fuente"}
            </button>
          </div>
        </div>
      </section>
      <AlbumShelf
        title="Recientes"
        groups={albumGroups(recent)}
        audio={audio}
        onBrowse={() => navigate("history")}
      />
      <AlbumShelf
        title="Añadidos recientemente"
        groups={albumGroups(added)}
        audio={audio}
        onBrowse={() => navigate("albums")}
      />
      <AlbumShelf
        title="Favoritos"
        groups={albumGroups(favorites)}
        audio={audio}
        onBrowse={() => navigate("favorites")}
      />
      {!!mostPlayed.length && (
        <section className="music-section">
          <div className="section-heading">
            <h2>Más escuchadas</h2>
          </div>
          <SongResults tracks={mostPlayed.slice(0, 5)} audio={audio} />
        </section>
      )}
    </div>
  );
}

function SongResults({
  tracks,
  audio,
}: {
  tracks: LibraryTrack[];
  audio: AudioController;
}) {
  return (
    <div className="discovery-songs">
      {tracks.map((t, i) => (
        <button
          key={t.id}
          className={`discovery-song ${audio.state.track?.trackId === t.id ? "current" : ""}`}
          disabled={!audio.available || !t.local_path || t.missing}
          onClick={() =>
            void audio.openEntries(
              playableTracks(tracks.slice(i)).map(entryFromTrack),
            )
          }
        >
          <span className="song-index">{i + 1}</span>
          <TrackArtwork
            className="library-cover"
            trackId={t.id}
            fallbackId={t.artwork_id}
          />
          <span className="song-info">
            <strong>{t.title}</strong>
            <span>{t.artist ?? "Artista desconocido"}</span>
          </span>
          <span className="song-album">{t.album ?? "Sin álbum"}</span>
          <span className="song-duration">
            {formatTime(t.duration === null ? null : t.duration * 1000)}
          </span>
          <Icon name="play" size={16} />
        </button>
      ))}
    </div>
  );
}

export function Search({
  audio,
  tracks,
  navigate,
  query,
  onQuery,
  onAlbum,
}: ScreenProps & {
  query: string;
  onQuery: (value: string) => void;
  onAlbum: (query: string, view: AppView) => void;
}) {
  const [filter, setFilter] = useState("Todo");
  const [submitted, setSubmitted] = useState<{
    query: string;
    revision: number;
    search: ReturnType<typeof searchExternalSources> | null;
  }>({ query: "", revision: 0, search: null });
  const lists = usePlaylists();
  const matched = searchMusic(tracks, query);
  const albums = albumGroups(matched);
  const artists = [
    ...new Map(matched.map((t) => [groupIdentity(t, "Artistas"), t])).values(),
  ];
  const playlists = query.trim()
    ? lists.playlists.filter((p) =>
        p.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()),
      )
    : [];
  return (
    <section className="search-screen">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (audio.available && query.trim())
            setSubmitted({
              query: query.trim(),
              revision: submitted.revision + 1,
              search: searchExternalSources(query.trim()),
            });
        }}
      >
        <label className="large-search">
          <Icon name="search" size={24} />
          <input
            autoFocus
            type="search"
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            placeholder="¿Qué quieres escuchar?"
            aria-label="Buscar música"
          />
        </label>
        <button
          className="button"
          type="submit"
          disabled={!audio.available || !query.trim()}
        >
          Buscar también en servicios
        </button>
      </form>
      <div className="filter-pills" aria-label="Filtrar búsqueda">
        {["Todo", "Canciones", "Álbumes", "Artistas", "Listas"].map((item) => (
          <button
            key={item}
            aria-pressed={filter === item}
            onClick={() => setFilter(item)}
          >
            {item}
          </button>
        ))}
      </div>
      {!query.trim() ? (
        <div className="search-invitation">
          <Icon name="search" size={42} />
          <h2>Busca tu música</h2>
          <p>
            Canciones, artistas, álbumes y listas. Pulsa Enter para consultar
            también tus servicios.
          </p>
        </div>
      ) : (
        <>
          {!matched.length && !playlists.length && (
            <div className="library-empty">
              <h2>Sin coincidencias locales para “{query}”</h2>
              <p>Prueba con otro nombre o busca también en tus servicios.</p>
            </div>
          )}
          {["Todo", "Canciones"].includes(filter) && !!matched.length && (
            <section className="music-section">
              <h2>
                Canciones <span className="result-count">{matched.length}</span>
              </h2>
              <SongResults tracks={matched} audio={audio} />
            </section>
          )}
          {["Todo", "Álbumes"].includes(filter) && !!albums.length && (
            <section className="music-section">
              <h2>Álbumes</h2>
              <div className="search-groups">
                {albums.map((items) => (
                  <button
                    key={items[0].id}
                    onClick={() => onAlbum(items[0].album ?? "", "albums")}
                  >
                    <TrackArtwork
                      className="library-cover"
                      trackId={items[0].id}
                    />
                    <span>
                      <strong>{items[0].album ?? "Sin álbum"}</strong>
                      <small>{items[0].artist}</small>
                    </span>
                  </button>
                ))}
              </div>
            </section>
          )}
          {["Todo", "Artistas"].includes(filter) && !!artists.length && (
            <section className="music-section">
              <h2>Artistas</h2>
              <div className="search-groups">
                {artists.map((artist) => (
                  <button
                    key={groupIdentity(artist, "Artistas")}
                    onClick={() => onAlbum(artist.artist ?? "", "artists")}
                  >
                    <Icon name="artists" />
                    <strong>{groupKey(artist, "Artistas")}</strong>
                  </button>
                ))}
              </div>
            </section>
          )}
          {["Todo", "Listas"].includes(filter) && (
            <section className="music-section">
              <h2>Listas</h2>
              {!playlists.length && (
                <p className="secondary">No hay listas con ese nombre.</p>
              )}
              <div className="search-groups">
                {playlists.map((list) => (
                  <button
                    key={list.id}
                    disabled={!audio.available || !list.entries.length}
                    onClick={() => void audio.openEntries(list.entries)}
                  >
                    <Icon name="playlists" />
                    <span>
                      <strong>{list.name}</strong>
                      <small>{list.entries.length} canciones</small>
                    </span>
                    <Icon name="play" size={16} />
                  </button>
                ))}
              </div>
            </section>
          )}
          {audio.available &&
            submitted.search &&
            submitted.query === query.trim() &&
            ["Todo", "Canciones"].includes(filter) && (
              <ExternalSearchResults
                key={submitted.revision}
                search={submitted.search}
                audio={audio}
              />
            )}
          <button
            className="external-search-link"
            onClick={() => navigate("sources")}
          >
            <Icon name="sources" />
            Buscar en fuentes conectadas <span>Elegir servicio</span>
          </button>
        </>
      )}
    </section>
  );
}

export function Downloads({
  audio,
  navigate,
}: {
  audio: AudioController;
  navigate: (view: AppView) => void;
}) {
  const [entries, setEntries] = useState<SourceEntry[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(audio.available);
  useEffect(() => {
    let disposed = false;
    if (audio.available)
      void sourcesAction({ kind: "downloads" })
        .then((snapshot) => {
          if (!disposed) setEntries(snapshot.results);
        })
        .catch((reason) => {
          if (!disposed) setError(String(reason));
        })
        .finally(() => {
          if (!disposed) setLoading(false);
        });
    return () => {
      disposed = true;
    };
  }, [audio.available]);
  return (
    <section>
      <p className="page-description">
        Canciones que elegiste descargar, disponibles sin conexión. Se eliminan
        al limpiar la caché.
      </p>
      {loading && <p role="status">Cargando descargas…</p>}
      {error && <p role="alert">{error}</p>}
      {!loading && !entries.length && (
        <div className="search-invitation">
          <Icon name="downloads" size={44} />
          <h2>Sin descargas</h2>
          <p>Descarga canciones desde tus fuentes.</p>
          <button
            className="button primary"
            onClick={() => navigate("sources")}
          >
            Explorar mis fuentes
          </button>
        </div>
      )}
      <div className="discovery-songs">
        {entries.map((entry) => (
          <button
            className="discovery-song"
            key={entry.id}
            onClick={() =>
              void audio.openEntries([
                { ...entryFromPath(entry.id), title: entry.name },
              ])
            }
          >
            <Icon name="music" />
            <span className="song-info">
              <strong>{entry.name}</strong>
              <span>Guardada en este equipo</span>
            </span>
            <Icon name="play" />
          </button>
        ))}
      </div>
    </section>
  );
}
