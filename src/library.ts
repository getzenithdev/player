import { invoke, isTauri } from "@tauri-apps/api/core";

export interface MetadataEdit {
  title: string;
  artist: string | null;
  album_artist: string | null;
  album: string | null;
  genre: string | null;
  year: number | null;
  track_number: number | null;
  disc_number: number | null;
  comments: string | null;
}
export interface LibraryTrack extends MetadataEdit {
  id: string;
  local_path: string | null;
  duration: number | null;
  artwork_id: string | null;
  format: string;
  size: number;
  added_at: number;
  missing: boolean;
  favorite: boolean;
  play_count: number;
  last_played: number | null;
}
export interface LibrarySnapshot {
  tracks: LibraryTrack[];
  locations: string[];
  duplicates: { ids: string[]; confirmed: boolean }[];
  warnings: string[];
}
export type LibraryAction =
  | { kind: "snapshot" | "rescan" }
  | { kind: "import"; paths: string[] }
  | { kind: "removeLocation"; path: string }
  | { kind: "favorite"; id: string; favorite: boolean }
  | { kind: "edit"; id: string; metadata: MetadataEdit; write_tags: boolean };
export const emptyLibrary: LibrarySnapshot = {
  tracks: [],
  locations: [],
  duplicates: [],
  warnings: [],
};
export async function libraryAction(
  action: LibraryAction,
): Promise<LibrarySnapshot> {
  if (!isTauri()) return emptyLibrary;
  return invoke<LibrarySnapshot>("library_action", { action });
}
export const libraryViews = [
  "Canciones",
  "Álbumes",
  "Artistas",
  "Géneros",
  "Carpetas",
  "Recientes",
  "Historial",
  "Más escuchadas",
  "Favoritos",
  "Duplicados",
] as const;
export type LibraryView = (typeof libraryViews)[number];
export function selectTracks(
  library: LibrarySnapshot,
  view: LibraryView,
  query: string,
): LibraryTrack[] {
  const needle = query.trim().toLocaleLowerCase();
  let tracks = library.tracks.filter((t) =>
    [
      t.title,
      t.artist,
      t.album_artist,
      t.album,
      t.genre,
      t.local_path,
      t.year,
    ].some((v) =>
      String(v ?? "")
        .toLocaleLowerCase()
        .includes(needle),
    ),
  );
  if (view === "Favoritos") tracks = tracks.filter((t) => t.favorite);
  if (view === "Historial" || view === "Más escuchadas")
    tracks = tracks.filter((t) => t.play_count > 0);
  if (view === "Duplicados") {
    const ids = new Set(library.duplicates.flatMap((g) => g.ids));
    tracks = tracks.filter((t) => ids.has(t.id));
  }
  return tracks.sort((a, b) => {
    if (view === "Recientes")
      return b.added_at - a.added_at || a.title.localeCompare(b.title);
    if (view === "Historial")
      return (b.last_played ?? 0) - (a.last_played ?? 0);
    if (view === "Más escuchadas") return b.play_count - a.play_count;
    const groupOrder = groupKey(a, view).localeCompare(groupKey(b, view));
    return (
      groupOrder ||
      (a.disc_number ?? 0) - (b.disc_number ?? 0) ||
      (a.track_number ?? 0) - (b.track_number ?? 0) ||
      a.title.localeCompare(b.title)
    );
  });
}
export function groupKey(track: LibraryTrack, view: LibraryView): string {
  switch (view) {
    case "Álbumes":
      return `${track.album ?? "Sin álbum"} · ${track.album_artist ?? track.artist ?? "Artista desconocido"}`;
    case "Artistas":
      return track.artist ?? "Artista desconocido";
    case "Géneros":
      return track.genre ?? "Sin género";
    case "Carpetas":
      return track.local_path?.replace(/[\\/][^\\/]*$/, "") ?? "Sin carpeta";
    default:
      return "";
  }
}
