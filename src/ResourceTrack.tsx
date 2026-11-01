import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AudioController } from "./audio";
import { libraryAction, type LibraryTrack } from "./library";
export function useResourceTrack(audio: AudioController) {
  const [tracks, setTracks] = useState<LibraryTrack[]>([]);
  const [chosen, setChosen] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    const reload = () =>
      void libraryAction({ kind: "snapshot" })
        .then((s) => {
          if (!disposed) setTracks(s.tracks);
        })
        .catch((e) => {
          if (!disposed) setError(String(e));
        });
    reload();
    const cleanup = isTauri()
      ? listen("library:changed", reload)
      : Promise.resolve(() => {});
    return () => {
      disposed = true;
      void cleanup.then((f) => f());
    };
  }, []);
  const current = audio.state.track;
  const track =
    tracks.find((t) => t.id === chosen) ??
    tracks.find(
      (t) => t.id === current?.trackId || t.local_path === current?.path,
    );
  return {
    tracks,
    track,
    error,
    selector: (
      <label className="resource-track">
        Canción
        <select
          value={chosen || ""}
          onChange={(e) => setChosen(e.target.value)}
        >
          <option value="">Seguir reproducción</option>
          {tracks.map((t) => (
            <option key={t.id} value={t.id}>
              {t.title} · {t.artist ?? "Sin artista"}
            </option>
          ))}
        </select>
      </label>
    ),
  };
}
