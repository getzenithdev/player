import { describe, expect, it } from "vitest";
import {
  emptyLibrary,
  groupKey,
  selectTracks,
  type LibraryTrack,
} from "./library";
const track = (
  id: string,
  patch: Partial<LibraryTrack> = {},
): LibraryTrack => ({
  id,
  title: id,
  artist: "Artist",
  album_artist: null,
  album: "Album",
  genre: "Rock",
  year: 2026,
  track_number: null,
  disc_number: null,
  comments: null,
  local_path: `/music/${id}.wav`,
  duration: 60,
  artwork_id: null,
  format: "WAV",
  size: 10,
  added_at: 1,
  missing: false,
  favorite: false,
  play_count: 0,
  last_played: null,
  ...patch,
});
describe("library views", () => {
  const library = {
    ...emptyLibrary,
    tracks: [
      track("a", { favorite: true, play_count: 2, last_played: 20 }),
      track("b", { play_count: 1, last_played: 30, added_at: 2 }),
      track("c", { missing: true }),
    ],
    duplicates: [{ ids: ["a", "c"], confirmed: true }],
  };
  it("filters favorites, preserves missing tracks and sorts actual history", () => {
    expect(selectTracks(library, "Favoritos", "").map((t) => t.id)).toEqual([
      "a",
    ]);
    expect(selectTracks(library, "Historial", "").map((t) => t.id)).toEqual([
      "b",
      "a",
    ]);
    expect(
      selectTracks(library, "Más escuchadas", "").map((t) => t.id),
    ).toEqual(["a", "b"]);
    expect(selectTracks(library, "Recientes", "")[0].id).toBe("b");
    expect(selectTracks(library, "Duplicados", "").map((t) => t.id)).toEqual([
      "a",
      "c",
    ]);
  });
  it("searches metadata, numbers and paths case-insensitively", () => {
    expect(selectTracks(library, "Canciones", "ARTIST")).toHaveLength(3);
    expect(
      selectTracks(library, "Canciones", "/music/b").map((t) => t.id),
    ).toEqual(["b"]);
    expect(selectTracks(library, "Canciones", "2026")).toHaveLength(3);
    expect(selectTracks(library, "Canciones", "nonexistent")).toEqual([]);
  });
  it("separates albums with different album artists and handles Windows folders", () => {
    expect(groupKey(track("a", { album_artist: "First" }), "Álbumes")).not.toBe(
      groupKey(track("b", { album_artist: "Second" }), "Álbumes"),
    );
    expect(
      groupKey(track("a", { local_path: "C:\\Music\\a.wav" }), "Carpetas"),
    ).toBe("C:\\Music");
  });
});
