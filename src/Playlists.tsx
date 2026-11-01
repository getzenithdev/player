import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { isTauri } from "@tauri-apps/api/core";
import { pickAudioFiles, type AudioController } from "./audio";
import { entryFromPath, moveTarget, usePlaylists } from "./playlists";
import "./Playlists.css";
export function Playlists({ audio }: { audio: AudioController }) {
  const lists = usePlaylists();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [format, setFormat] = useState("m3u8");
  const [dialogBusy, setDialogBusy] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const selected =
    lists.playlists.find((p) => p.id === selectedId) ?? lists.playlists[0];
  const native = isTauri();
  const busy = lists.busy || dialogBusy;
  async function dialog(action: () => Promise<void>) {
    setDialogBusy(true);
    setDialogError(null);
    try {
      await action();
    } catch (reason) {
      setDialogError(String(reason));
    } finally {
      setDialogBusy(false);
    }
  }
  return (
    <section className="playlists-panel" aria-busy={busy}>
      <p className="page-description">
        Listas guardadas en este equipo. Sus canciones y orden son
        independientes de la cola.
      </p>
      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          void lists.run({ kind: "create", name }).then((result) => {
            if (result) {
              setSelectedId(
                result.playlists[result.playlists.length - 1]?.id ?? null,
              );
              setName("");
            }
          });
        }}
      >
        <label className="search-field">
          <span className="sr-only">Nombre de la nueva lista</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={128}
            required
            placeholder="Nueva lista"
          />
        </label>
        <button
          className="button primary"
          disabled={!native || busy || !name.trim()}
        >
          Crear lista
        </button>
        <button
          type="button"
          className="button"
          disabled={!native || busy}
          onClick={() =>
            void dialog(async () => {
              const path = await open({
                multiple: false,
                directory: false,
                filters: [
                  { name: "Listas", extensions: ["m3u", "m3u8", "json"] },
                ],
              });
              if (typeof path === "string")
                await lists.run({ kind: "import", path });
            })
          }
        >
          Importar lista
        </button>
      </form>
      {!native && (
        <p className="inline-note">
          La gestión de listas requiere la aplicación de escritorio.
        </p>
      )}
      {(lists.error || dialogError) && (
        <p role="alert" className="inline-note error-note">
          {lists.error || dialogError}
        </p>
      )}
      {lists.notice && (
        <p role="status" className="inline-note">
          {lists.notice}
        </p>
      )}
      <nav className="playlist-picker" aria-label="Listas guardadas">
        {lists.playlists.map((p) => (
          <button
            className={`button ${p.id === selected?.id ? "active" : ""}`}
            key={p.id}
            aria-pressed={p.id === selected?.id}
            onClick={() => setSelectedId(p.id)}
          >
            {p.name} · {p.entries.length}
          </button>
        ))}
      </nav>
      {selected ? (
        <>
          <form
            key={selected.id + selected.name}
            className="toolbar"
            onSubmit={(e) => {
              e.preventDefault();
              const data = new FormData(e.currentTarget);
              void lists.run({
                kind: "rename",
                id: selected.id,
                name: String(data.get("name") ?? ""),
              });
            }}
          >
            <label className="search-field">
              <span className="sr-only">Nombre de la lista seleccionada</span>
              <input
                name="name"
                defaultValue={selected.name}
                maxLength={128}
                required
              />
            </label>
            <button className="button" disabled={busy}>
              Renombrar
            </button>
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() =>
                void lists.run({ kind: "duplicate", id: selected.id })
              }
            >
              Duplicar
            </button>
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() =>
                void lists.run({ kind: "delete", id: selected.id })
              }
            >
              Eliminar lista
            </button>
          </form>
          <div className="toolbar">
            <button
              className="button primary"
              disabled={busy || !selected.entries.some((e) => e.available)}
              onClick={() => void audio.openEntries(selected.entries, true)}
            >
              Reproducir lista
            </button>
            <button
              className="button"
              disabled={busy}
              onClick={() =>
                void dialog(async () => {
                  const paths = await pickAudioFiles();
                  if (paths.length)
                    await lists.run({
                      kind: "add",
                      id: selected.id,
                      entries: paths.map(entryFromPath),
                    });
                })
              }
            >
              Añadir archivos
            </button>
            <label>
              Formato de exportación{" "}
              <select
                value={format}
                onChange={(e) => setFormat(e.target.value)}
              >
                <option value="m3u8">M3U8</option>
                <option value="m3u">M3U</option>
                <option value="json">JSON</option>
              </select>
            </label>
            <button
              className="button"
              disabled={busy}
              onClick={() =>
                void dialog(async () => {
                  const path = await save({
                    title: "Exportar lista",
                    defaultPath: `${selected.name.replace(/[\\/:*?"<>|]/g, "_")}.${format}`,
                    filters: [
                      { name: format.toUpperCase(), extensions: [format] },
                    ],
                  });
                  if (path)
                    await lists.run({
                      kind: "export",
                      id: selected.id,
                      path,
                      format,
                    });
                })
              }
            >
              Exportar lista
            </button>
          </div>
          <ul className="plain-list">
            {selected.entries.map((entry, index) => (
              <li key={entry.id}>
                <span>{index + 1}</span>
                <button
                  className="queue-track"
                  disabled={!entry.available || busy}
                  onClick={() =>
                    void audio.openEntries(
                      [
                        ...selected.entries.slice(index),
                        ...selected.entries.slice(0, index),
                      ],
                      true,
                    )
                  }
                >
                  <strong>{entry.title}</strong>
                  <span className="secondary">
                    {entry.available
                      ? entry.path
                      : "Archivo no disponible · " + entry.path}
                  </span>
                </button>
                <div className="button-group">
                  {([-1, 1] as const).map((direction) => (
                    <button
                      key={direction}
                      className="button small"
                      aria-label={`${direction === -1 ? "Subir" : "Bajar"} ${entry.title}`}
                      disabled={
                        busy ||
                        moveTarget(selected.entries, index, direction) ===
                          undefined
                      }
                      onClick={() =>
                        void lists.run({
                          kind: "move",
                          id: selected.id,
                          entry_id: entry.id,
                          before_id:
                            moveTarget(selected.entries, index, direction) ??
                            null,
                        })
                      }
                    >
                      {direction === -1 ? "↑" : "↓"}
                    </button>
                  ))}
                  <button
                    className="button small"
                    disabled={busy}
                    onClick={() =>
                      void lists.run({
                        kind: "remove",
                        id: selected.id,
                        entry_id: entry.id,
                      })
                    }
                  >
                    Quitar
                  </button>
                </div>
              </li>
            ))}
          </ul>
          {!selected.entries.length && (
            <p className="inline-note">
              Añade archivos aquí o usa “Añadir a lista” desde la biblioteca.
            </p>
          )}
        </>
      ) : (
        <div className="library-empty">
          <h2>Todavía no hay listas</h2>
          <p>Crea una lista o importa M3U, M3U8 o JSON.</p>
        </div>
      )}
    </section>
  );
}
