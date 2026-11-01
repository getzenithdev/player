import { useState } from "react";
import { pickAudioFiles, type AudioController } from "./audio";
import { moveTarget, usePlaylists } from "./playlists";
import "./Playlists.css";
const playbackLabels = {
  idle: "En espera",
  loading: "Cargando",
  playing: "Reproduciendo",
  paused: "En pausa",
  stopped: "Detenido",
  error: "Error",
};
export function QueuePanel({ audio }: { audio: AudioController }) {
  const { state } = audio;
  const lists = usePlaylists();
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  async function files(mode: "replace" | "append" | "next") {
    setBusy(true);
    try {
      const paths = await pickAudioFiles();
      if (paths.length) {
        if (mode === "replace") await audio.openFiles(paths, true);
        else
          await audio.queueAction({
            kind: "append",
            paths,
            after_current: mode === "next",
          });
      }
    } catch (reason) {
      audio.reportError(reason);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="queue-panel" aria-busy={busy || lists.busy}>
      <p className="page-description">
        {state.queue.length} entradas. La cola se guarda automáticamente, con la
        canción y la posición, y se restaura sin comenzar a reproducir.
      </p>
      <div className="toolbar">
        {(
          [
            ["replace", "Abrir y reemplazar cola"],
            ["append", "Añadir al final"],
            ["next", "Reproducir después"],
          ] as const
        ).map(([mode, label]) => (
          <button
            className="button"
            key={mode}
            disabled={!audio.available || busy}
            onClick={() => void files(mode)}
          >
            {label}
          </button>
        ))}
        <button
          className="button"
          disabled={!audio.available || !state.queue.length}
          onClick={() => void audio.clearQueue()}
        >
          Vaciar cola
        </button>
      </div>
      <div className="toolbar">
        <button
          className={`button ${state.shuffle ? "active" : ""}`}
          aria-pressed={state.shuffle}
          disabled={!audio.available}
          onClick={() => void audio.setShuffle(!state.shuffle)}
        >
          Aleatorio
        </button>
        <label>
          Repetición{" "}
          <select
            value={state.repeat}
            disabled={!audio.available}
            onChange={(e) =>
              void audio.setRepeat(e.target.value as "off" | "one" | "all")
            }
          >
            <option value="off">Desactivada</option>
            <option value="all">Toda la cola</option>
            <option value="one">Una canción</option>
          </select>
        </label>
        <button
          className="button"
          disabled={!audio.available}
          onClick={() => void audio.queueAction({ kind: "save" })}
        >
          Guardar copia de la cola
        </button>
        <button
          className="button"
          disabled={!audio.available}
          onClick={() => void audio.queueAction({ kind: "restore" })}
        >
          Restaurar copia guardada
        </button>
        <button
          className="button"
          disabled={!audio.available}
          onClick={() => void audio.restartOutput()}
        >
          Reiniciar salida
        </button>
      </div>
      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          void lists
            .run({ kind: "create", name, entries: state.queue })
            .then((result) => {
              if (result) {
                setName("");
                setNotice("Cola guardada como una nueva lista independiente");
              }
            });
        }}
      >
        <label className="search-field">
          <span className="sr-only">
            Nombre para guardar la cola como lista
          </span>
          <input
            value={name}
            maxLength={128}
            placeholder="Guardar cola como lista"
            onChange={(e) => setName(e.target.value)}
            required
          />
        </label>
        <button
          className="button"
          disabled={
            !audio.available ||
            lists.busy ||
            !state.queue.length ||
            !name.trim()
          }
        >
          Guardar como lista
        </button>
      </form>
      {(audio.notice || notice) && (
        <p role="status" className="inline-note">
          {notice || audio.notice}
        </p>
      )}
      {(audio.error || state.error || lists.error) && (
        <p role="alert" className="inline-note error-note">
          {audio.error || state.error || lists.error}
        </p>
      )}
      <ul className="plain-list queue-list">
        {state.queue.map((entry, index) => (
          <li
            className={state.currentIndex === index ? "current" : ""}
            key={entry.id}
          >
            <span className="queue-index">
              {state.currentIndex === index ? "▶" : index + 1}
            </span>
            <button
              className="queue-track"
              disabled={!audio.available || !entry.available}
              onClick={() => void audio.playIndex(index)}
            >
              <strong>{entry.title}</strong>
              <span className="secondary">
                {entry.available ? entry.format : "Archivo no disponible"}
                {state.currentIndex === index
                  ? " · " + playbackLabels[state.status]
                  : ""}
              </span>
            </button>
            <div className="button-group">
              {([-1, 1] as const).map((direction) => (
                <button
                  key={direction}
                  className="button small"
                  aria-label={`${direction === -1 ? "Subir" : "Bajar"} ${entry.title}`}
                  disabled={
                    !audio.available ||
                    moveTarget(state.queue, index, direction) === undefined
                  }
                  onClick={() =>
                    void audio.queueAction({
                      kind: "move",
                      id: entry.id,
                      before_id:
                        moveTarget(state.queue, index, direction) ?? null,
                    })
                  }
                >
                  {direction === -1 ? "↑" : "↓"}
                </button>
              ))}
              <button
                className="button small"
                disabled={!audio.available}
                onClick={() =>
                  void audio.queueAction({ kind: "remove", id: entry.id })
                }
              >
                Quitar de cola
              </button>
            </div>
          </li>
        ))}
      </ul>
      {!state.queue.length && (
        <div className="library-empty">
          <h2>No hay canciones en espera</h2>
          <p>
            Agrega música desde la biblioteca, una lista o archivos locales.
          </p>
        </div>
      )}
      <p className="inline-note">
        “Reproducir después” inserta las canciones inmediatamente después de la
        actual. “Añadir al final” conserva todas las entradas que ya estaban
        esperando. Las entradas ausentes se conservan y se saltan al avanzar.
      </p>
    </section>
  );
}
