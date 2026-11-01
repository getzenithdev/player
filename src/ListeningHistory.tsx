import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AudioController } from "./audio";
import type { HistoryVisit } from "./playlists";
export function ListeningHistory({ audio }: { audio: AudioController }) {
  const [visits, setVisits] = useState<HistoryVisit[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    let generation = 0;
    let unlisten: (() => void) | undefined;
    async function reload() {
      const request = ++generation;
      try {
        const result = await invoke<{ visits: HistoryVisit[] }>(
          "history_snapshot",
        );
        if (!disposed && request === generation)
          setVisits([...result.visits].reverse());
      } catch (reason) {
        if (!disposed) setError(String(reason));
      }
    }
    if (isTauri())
      void (async () => {
        const cleanup = await listen("library:changed", () => {
          void reload();
        });
        if (disposed) cleanup();
        else unlisten = cleanup;
        if (!disposed) await reload();
      })().catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  return (
    <section>
      <p className="page-description">
        Últimas 1000 escuchas. Una reproducción cuenta después de 30 segundos de
        audio efectivo; las escuchas más cortas también se conservan con su
        tiempo. Pausar o buscar una posición no suma tiempo.
      </p>
      {error && (
        <p role="alert" className="inline-note error-note">
          {error}
        </p>
      )}
      <ul className="plain-list">
        {visits.map((v) => (
          <li key={v.id}>
            <div className="history-row">
              <strong>{v.entry.title}</strong>
              <span className="secondary">
                {new Date(v.startedAt * 1000).toLocaleString()} ·{" "}
                {Math.floor(v.listenedMs / 1000)} segundos escuchados ·{" "}
                {v.listenedMs >= 30000
                  ? "Reproducción contabilizada"
                  : "Escucha corta"}
              </span>
            </div>
            <button
              className="button small"
              disabled={!v.entry.available}
              onClick={() => void audio.openEntries([v.entry], true)}
            >
              Reproducir
            </button>
          </li>
        ))}
      </ul>
      {!visits.length && (
        <div className="library-empty">
          <h2>Todavía no hay escuchas registradas</h2>
          <p>
            El historial se guarda durante la reproducción en la aplicación de
            escritorio.
          </p>
        </div>
      )}
    </section>
  );
}
