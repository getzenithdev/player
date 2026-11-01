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
export interface MetadataRecord {
  version: number;
  original: {
    title: string;
    artist: string | null;
    album_artist: string | null;
    album: string | null;
    genre: string | null;
    comments: string | null;
  } | null;
  normalized: {
    title: string;
    artist: string;
    album_artist: string;
    album: string;
    genre: string;
  };
  identifiers: {
    musicbrainz_recording_id?: string | null;
    musicbrainz_release_id?: string | null;
    musicbrainz_release_group_id?: string | null;
    musicbrainz_artist_id?: string | null;
    musicbrainz_album_artist_id?: string | null;
    isrc?: string | null;
    provider_track_ids?: Record<string, string>;
    provider_artist_ids?: Record<string, string>;
    provider_album_ids?: Record<string, string>;
  };
  edition?: string | null;
  track_version?: string | null;
  remixer?: string | null;
  artist_disambiguation?: string | null;
  qualifiers: string[];
  artist_key: string;
  album_key: string;
  provider_data: Record<string, unknown>;
}
export function metadataKey(value: string): string {
  return value
    .normalize("NFC")
    .trim()
    .replace(/\s+/gu, " ")
    .toLowerCase()
    .normalize("NFC");
}
export function groupIdentity(track: LibraryTrack, view: LibraryView): string {
  if (view === "Álbumes" && track.metadata?.album_key)
    return track.metadata.album_key;
  if (view === "Artistas" && track.metadata?.artist_key)
    return track.metadata.artist_key;
  return metadataKey(groupKey(track, view));
}
export interface LibraryTrack extends MetadataEdit {
  id: string;
  source?: string | null;
  source_id?: string | null;
  metadata?: MetadataRecord;
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
  listened_ms?: number;
  last_position_ms?: number;
}
export interface LibrarySnapshot {
  tracks: LibraryTrack[];
  locations: string[];
  duplicates: { ids: string[]; confirmed: boolean }[];
  warnings: string[];
  metadata_analysis?: {
    album_candidates: {
      name: string;
      album_keys: string[];
      track_ids: string[];
    }[];
    artist_collisions: { name: string; identities: string[] }[];
  };
}
export type LibraryAction =
  | { kind: "snapshot" | "rescan" }
  | { kind: "importMetadata"; path: string }
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
  const needle = metadataKey(query);
  let tracks = library.tracks.filter((t) =>
    [
      t.title,
      t.artist,
      t.album_artist,
      t.album,
      t.genre,
      t.local_path,
      t.year,
      t.source,
      t.source_id,
      ...Object.values(t.metadata?.identifiers ?? {}).flatMap((value) =>
        typeof value === "string"
          ? [value]
          : value && typeof value === "object"
            ? Object.values(value)
            : [],
      ),
    ].some((v) => metadataKey(String(v ?? "")).includes(needle)),
  );
  if (view === "Favoritos") tracks = tracks.filter((t) => t.favorite);
  if (view === "Historial")
    tracks = tracks.filter((t) => t.last_played !== null || t.play_count > 0);
  if (view === "Más escuchadas")
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
    const groupOrder =
      groupKey(a, view).localeCompare(groupKey(b, view)) ||
      groupIdentity(a, view).localeCompare(groupIdentity(b, view));
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
      return (
        (track.artist ?? "Artista desconocido") +
        (track.metadata?.artist_disambiguation
          ? ` · ${track.metadata.artist_disambiguation}`
          : "")
      );
    case "Géneros":
      return track.genre ?? "Sin género";
    case "Carpetas":
      return track.local_path?.replace(/[\\/][^\\/]*$/, "") ?? "Sin carpeta";
    default:
      return "";
  }
}
