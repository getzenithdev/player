import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { libraryAction, emptyLibrary, type LibraryTrack } from "./library";
import {
  favoriteSource,
  sourceLibraryTrack,
  type SourceEntry,
} from "./sources";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./library", async (original) => ({
  ...(await original<typeof import("./library")>()),
  libraryAction: vi.fn(),
}));
beforeEach(() => vi.resetAllMocks());
const entry: SourceEntry = {
  provider: "webdav",
  id: "song.wav",
  name: "Song",
  kind: "audio",
  detail: "",
  size: null,
  source_url: null,
};
const track: LibraryTrack = {
  id: "saved",
  title: "Song", artist: null, album_artist: null, album: null, genre: null,
  year: null, track_number: null, disc_number: null, comments: null,
  duration: null, artwork_id: null, format: "wav", size: 0, added_at: 0,
  missing: false, play_count: 0, last_played: null,
  local_path: "/cache/song.wav",
  favorite: true,
  metadata: {
    version: 1, original: null,
    normalized: { title: "song", artist: "", album_artist: "", album: "", genre: "" },
    qualifiers: [], artist_key: "", album_key: "", provider_data: {},
    identifiers: { provider_track_ids: { webdav: entry.id } },
  },
};

it("removes an existing favorite without downloading it again", async () => {
  vi.mocked(libraryAction).mockResolvedValue(emptyLibrary);
  await favoriteSource(entry, [track]);
  expect(invoke).not.toHaveBeenCalled();
  expect(libraryAction).toHaveBeenCalledWith({
    kind: "favorite",
    id: "saved",
    favorite: false,
  });
});

it.each([
  "youtube",
  "youtube-auto",
  "google-drive",
  "dropbox",
  "onedrive",
  "webdav",
] as const)("prepares and favorites a new %s result", async (provider) => {
  vi.mocked(invoke).mockResolvedValue({ downloaded: track.local_path });
  vi.mocked(libraryAction).mockResolvedValue({
    ...emptyLibrary,
    tracks: [{ ...track, favorite: false }],
  });
  await favoriteSource({ ...entry, provider }, []);
  expect(invoke).toHaveBeenCalledWith("sources_action", {
    action: { kind: "prepare", provider, id: entry.id },
  });
  expect(libraryAction).toHaveBeenLastCalledWith({
    kind: "favorite",
    id: "saved",
    favorite: true,
  });
});

it("keeps provider IDs separate and recognizes both YouTube adapters", () => {
  expect(
    sourceLibraryTrack([track], { ...entry, provider: "dropbox" }),
  ).toBeUndefined();
  const youtube = {
    ...track,
    metadata: {
      ...track.metadata!,
      identifiers: { provider_track_ids: { youtube: "video" } },
    },
  };
  expect(
    sourceLibraryTrack([youtube], {
      ...entry,
      provider: "youtube-auto",
      id: "video",
    }),
  ).toBe(youtube);
});

it("does not save a favorite if preparing audio fails", async () => {
  vi.mocked(invoke).mockRejectedValue(new Error("Sin conexión"));
  await expect(favoriteSource(entry, [])).rejects.toThrow("Sin conexión");
  expect(libraryAction).not.toHaveBeenCalled();
});
