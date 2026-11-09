import { useEffect, useMemo, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AudioController } from "./audio";
import { libraryAction, type LibraryTrack } from "./library";
export function useResourceTrack(audio: AudioController) {
  const [tracks, setTracks] = useState<LibraryTrack[]>([]);
  const [chosen, setChosen] = useState("");
  const [error, setError] = useState("");
  const current = audio.state.track;
  useEffect(() => {
    let disposed = false;
    const reload = () =>
      void libraryAction({ kind: "snapshot" })
        .then((s) => {
          if (!disposed) {
            setTracks(s.tracks);
            setError("");
          }
        })
        .catch((e) => {
          if (!disposed) setError(String(e));
        });
    const cleanup = isTauri()
      ? listen("library:changed", reload)
      : Promise.resolve(() => {});
    void cleanup.then(() => {
      if (!disposed) reload();
    });
    return () => {
      disposed = true;
      void cleanup.then((f) => f());
    };
  }, [current?.trackId, current?.path]);
  const track = useMemo(
    () =>
      tracks.find((t) => t.id === chosen) ??
      tracks.find((t) => !!current?.trackId && t.id === current.trackId) ??
      tracks.find((t) => !!current?.path && t.local_path === current.path),
    [tracks, chosen, current?.trackId, current?.path],
  );
  const selector = useMemo(
    () => (
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
    [tracks, chosen],
  );
  return { tracks, track, error, selector };
}
