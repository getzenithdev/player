import { describe, expect, it } from "vitest";
import {
  emptyLibrary,
  groupKey,
  groupIdentity,
  metadataKey,
  type MetadataRecord,
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

it("shows short listening visits in history without treating them as counted plays", () => {
  const t = {
    id: "short",
    title: "Short",
    artist: null,
    album: null,
    album_artist: null,
    genre: null,
    year: null,
    track_number: null,
    disc_number: null,
    comments: null,
    local_path: "/music/short.wav",
    duration: 10,
    artwork_id: null,
    format: "WAV",
    size: 10,
    added_at: 1,
    missing: false,
    favorite: false,
    play_count: 0,
    last_played: 123,
  } satisfies LibraryTrack;
  const library = { ...emptyLibrary, tracks: [t] };
  expect(selectTracks(library, "Historial", "")).toHaveLength(1);
  expect(selectTracks(library, "Más escuchadas", "")).toHaveLength(0);
});

describe("universal metadata presentation", () => {
  const metadata = (artist_key: string, album_key: string): MetadataRecord => ({
    version: 1,
    original: null,
    normalized: {
      title: "song",
      artist: "same",
      album_artist: "same",
      album: "album",
      genre: "",
    },
    identifiers: {
      musicbrainz_recording_id: "12345678-1234-1234-1234-123456789abc",
      provider_track_ids: { apple_music: "apple-123" },
    },
    qualifiers: [],
    artist_key,
    album_key,
    provider_data: {},
  });
  it("normalizes canonical Unicode and repeated whitespace for search", () => {
    expect(metadataKey("  Cafe\u0301 \t LIVE ")).toBe("café live");
    const library = {
      ...emptyLibrary,
      tracks: [track("one", { title: "Cafe\u0301 Live" })],
    };
    expect(selectTracks(library, "Canciones", " CAFÉ   LIVE ")).toHaveLength(1);
  });
  it("does not group same-name artists or albums with different stable keys", () => {
    const a = track("a", { metadata: metadata("artist:one", "release:one") });
    const b = track("b", { metadata: metadata("artist:two", "release:two") });
    expect(groupIdentity(a, "Artistas")).not.toBe(groupIdentity(b, "Artistas"));
    expect(groupIdentity(a, "Álbumes")).not.toBe(groupIdentity(b, "Álbumes"));
    expect(groupKey(a, "Álbumes")).toBe(groupKey(b, "Álbumes"));
  });
  it("searches retained recording and provider identifiers without requiring a local file", () => {
    const t = track("cloud", {
      local_path: null,
      source: "apple_music",
      source_id: "source-item",
      metadata: metadata("artist:one", "release:one"),
    });
    const library = { ...emptyLibrary, tracks: [t] };
    expect(selectTracks(library, "Canciones", "apple-123")).toHaveLength(1);
    expect(
      selectTracks(
        library,
        "Canciones",
        "12345678-1234-1234-1234-123456789abc",
      ),
    ).toHaveLength(1);
    expect(selectTracks(library, "Canciones", "source-item")).toHaveLength(1);
  });
});
