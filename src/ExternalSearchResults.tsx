import { useEffect, useState } from "react";
import type { AudioController } from "./audio";
import { SourceArtwork } from "./SourceArtwork";
import {
  entryActions,
  searchExternalSources,
  sourcesAction,
  preparedSourceEntry,
  type SourceEntry,
} from "./sources";
import "./Resources.css";

export function ExternalSearchResults({
  search,
  audio,
}: {
  search: ReturnType<typeof searchExternalSources>;
  audio: AudioController;
}) {
  const [result, setResult] = useState<Awaited<
    ReturnType<typeof searchExternalSources>
  > | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let disposed = false;
    void search
      .then((next) => {
        if (!disposed) setResult(next);
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, [search]);

  async function act(entry: SourceEntry, play: boolean) {
    setBusy(true);
    setError("");
    try {
      const next = await sourcesAction({
        kind: play ? "prepare" : "download",
        provider: entry.provider,
        id: entry.id,
        name: entry.name,
      });
      if (play && next.downloaded)
        await audio.openEntries([
          await preparedSourceEntry(next.downloaded, entry),
        ]);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="music-section">
      <h2>Servicios externos</h2>
      {!result && !error && <p role="status">Buscando en tus servicios…</p>}
      {result && !result.external && (
        <p>
          Activa las consultas externas en Fuentes para buscar en los servicios.
        </p>
      )}
      {result?.external &&
        !result.results.length &&
        !result.warnings.length && (
          <p>No hay coincidencias en los servicios disponibles.</p>
        )}
      {result?.warnings.map((warning, index) => (
        <p role="status" key={index}>
          {warning}
        </p>
      ))}
      {error && <p role="alert">{error}</p>}
      {busy && <p role="status">Preparando audio…</p>}
      <ul className="resource-results">
        {result?.results.map((entry) => {
          const provider = result.providers.find(
            (provider) => provider.id === entry.provider,
          );
          const actions = provider && entryActions(provider, entry);
          return (
            <li key={`${entry.provider}:${entry.id}`}>
              <SourceArtwork entry={entry} />
              <div className="grow">
                <strong>{entry.name}</strong>
                <p className="secondary">
                  {entry.detail} · {provider?.name}
                </p>
              </div>
              <div className="resource-actions">
                {actions?.play && (
                  <button
                    className="button small"
                    disabled={busy}
                    onClick={() => void act(entry, true)}
                  >
                    Reproducir
                  </button>
                )}
                {actions?.download && (
                  <button
                    className="button small"
                    disabled={busy}
                    onClick={() => void act(entry, false)}
                  >
                    Descargar
                  </button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
