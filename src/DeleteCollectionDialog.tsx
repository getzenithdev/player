import { useEffect, useRef, useState } from "react";

export function DeleteCollectionDialog({
  kind,
  title,
  trackIds,
  onDelete,
  onClose,
}: {
  kind: "album" | "artist";
  title: string;
  trackIds: string[];
  onDelete: (ids: string[]) => Promise<void>;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    const element = dialog.current;
    const opener = document.activeElement;
    element?.showModal();
    return () => {
      element?.close();
      if (opener instanceof HTMLElement && opener.isConnected)
        opener.focus({ preventScroll: true });
    };
  }, []);
  async function remove() {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await onDelete(trackIds);
    } catch (reason) {
      setError(String(reason));
      setBusy(false);
    }
  }
  return (
    <dialog
      ref={dialog}
      className="delete-collection-dialog"
      aria-labelledby="delete-collection-title"
      aria-describedby="delete-collection-description"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
    >
      <h2 id="delete-collection-title">
        ¿Eliminar {kind === "album" ? "álbum" : "artista"} de la biblioteca?
      </h2>
      <p className="delete-collection-name">{title}</p>
      <div id="delete-collection-description">
        <p>
          Se{" "}
          {trackIds.length === 1
            ? "eliminará 1 canción relacionada"
            : `eliminarán ${trackIds.length} canciones relacionadas`}{" "}
          de la biblioteca, junto con sus favoritos, referencias en listas,
          historial, letras y portadas asociadas.
        </p>
        {kind === "artist" && (
          <p>
            Se incluyen las canciones del artista y las de los álbumes que
            figuran a su nombre. En álbumes compartidos se conservan las
            canciones de otros artistas.
          </p>
        )}
        <p>
          Los archivos de música del disco se conservan. La exploración
          automática no volverá a añadir estas canciones; podrás importarlas de
          nuevo cuando quieras.
        </p>
      </div>
      {error && (
        <p className="error-note" role="alert">
          {error}
        </p>
      )}
      <footer>
        <button autoFocus className="button" disabled={busy} onClick={onClose}>
          Cancelar
        </button>
        <button
          className="button collection-delete-confirm"
          disabled={busy}
          onClick={() => void remove()}
        >
          {busy ? "Eliminando…" : "Eliminar de la biblioteca"}
        </button>
      </footer>
    </dialog>
  );
}
