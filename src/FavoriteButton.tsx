import { useState } from "react";
import { Icon } from "./Icon";
import { libraryAction, type LibraryTrack } from "./library";
import { favoriteSource, type SourceEntry } from "./sources";

export function FavoriteButton({
  track,
  source,
  tracks = [],
  disabled = false,
}: {
  track?: LibraryTrack;
  source?: SourceEntry;
  tracks?: LibraryTrack[];
  disabled?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const favorite = track?.favorite ?? false;
  async function toggle() {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      if (source) await favoriteSource(source, tracks);
      else if (track)
        await libraryAction({
          kind: "favorite",
          id: track.id,
          favorite: !favorite,
        });
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <span className="search-favorite">
      <button
        type="button"
        className="icon-button"
        aria-pressed={favorite}
        aria-label={`${favorite ? "Quitar de" : "Agregar a"} favoritos: ${source?.name ?? track?.title ?? "audio"}`}
        title={
          busy
            ? "Guardando favorito…"
            : favorite
              ? "Quitar de favoritos"
              : "Agregar a favoritos"
        }
        disabled={disabled || busy}
        onClick={() => void toggle()}
      >
        <Icon name="heart" size={18} />
      </button>
      {busy && (
        <span className="sr-only" role="status">
          Guardando favorito…
        </span>
      )}
      {error && <span role="alert">{error}</span>}
    </span>
  );
}
