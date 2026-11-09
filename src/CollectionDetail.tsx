import { useState } from "react";
import { createPortal } from "react-dom";
import { isTauri } from "@tauri-apps/api/core";
import {
  ArtistArtwork,
  chooseArtistPhoto,
  useArtistPhoto,
} from "./ArtistArtwork";
import { TrackArtwork } from "./TrackArtwork";
import { Icon } from "./Icon";
import { groupIdentity, type LibraryTrack } from "./library";
import "./CollectionDetail.css";

export function CollectionDetail({
  kind,
  tracks,
  artist,
  onArtist,
  onPlay,
  backgroundTarget,
}: {
  kind: "album" | "artist";
  tracks: LibraryTrack[];
  artist: LibraryTrack;
  onArtist: () => void;
  onPlay: () => void;
  backgroundTarget?: HTMLElement | null;
}) {
  const first = tracks[0];
  const name = artist.artist ?? "Artista desconocido";
  const artistKey = groupIdentity(artist, "Artistas");
  const photo = useArtistPhoto(artistKey, artist.id, true, name);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const title = kind === "album" ? (first.album ?? "Sin álbum") : name;
  const duration = tracks.reduce(
    (total, track) => total + (track.duration ?? 0),
    0,
  );
  async function changePhoto() {
    setBusy(true);
    setError("");
    try {
      await chooseArtistPhoto(artistKey, artist.id);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  const atmosphere = (
    <div className="collection-atmosphere" aria-hidden="true">
      {kind === "artist" && photo ? (
        <img src={photo} alt="" decoding="async" />
      ) : (
        <TrackArtwork
          className="collection-background-art"
          trackId={first.id}
          fallbackId={first.artwork_id}
          quality="preview"
        />
      )}
    </div>
  );
  return (
    <header className={`collection-detail collection-detail-${kind}`}>
      {backgroundTarget
        ? createPortal(atmosphere, backgroundTarget)
        : atmosphere}
      <div className="collection-detail-body">
        {kind === "album" ? (
          <TrackArtwork
            className="collection-featured-cover"
            trackId={first.id}
            fallbackId={first.artwork_id}
            quality="preview"
          />
        ) : (
          <ArtistArtwork
            className="collection-featured-artist"
            artistKey={artistKey}
            name={name}
            trackId={artist.id}
          />
        )}
        <div className="collection-detail-copy">
          <p className="collection-kind">
            {kind === "album" ? "Álbum" : "Artista"}
          </p>
          <h2>{title}</h2>
          {kind === "album" && (
            <button className="collection-artist-link" onClick={onArtist}>
              <ArtistArtwork
                className="collection-artist-avatar"
                artistKey={artistKey}
                name={name}
                trackId={artist.id}
              />
              <span>{name}</span>
            </button>
          )}
          {kind === "artist" && artist.metadata?.artist_disambiguation && (
            <p className="collection-subtitle">
              {artist.metadata.artist_disambiguation}
            </p>
          )}
          <p className="collection-facts">
            {kind === "album" && first.year ? (
              <>
                <span>{first.year}</span>
                <span aria-hidden="true">·</span>
              </>
            ) : null}
            <span>
              {tracks.length} {tracks.length === 1 ? "canción" : "canciones"}
            </span>
            {duration > 0 && (
              <>
                <span aria-hidden="true">·</span>
                <span>{Math.round(duration / 60)} min</span>
              </>
            )}
          </p>
          <div className="collection-detail-actions">
            <button
              className="button primary"
              onClick={onPlay}
              disabled={
                !tracks.some((track) => track.local_path && !track.missing)
              }
            >
              <Icon name="play" size={18} /> Reproducir
            </button>
            <button
              className="text-button collection-photo-button"
              onClick={() => void changePhoto()}
              disabled={!isTauri() || busy}
            >
              {busy
                ? "Guardando foto…"
                : photo
                  ? "Cambiar foto del artista"
                  : "Añadir foto del artista"}
            </button>
          </div>
          {error && (
            <p className="error-note" role="alert">
              {error}
            </p>
          )}
        </div>
      </div>
    </header>
  );
}
