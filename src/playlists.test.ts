import { describe, it, expect } from "vitest";
import {
  entryFromPath,
  entryFromTrack,
  moveTarget,
  type MediaEntry,
} from "./playlists";
import type { LibraryTrack } from "./library";
describe("playlist and queue commands", () => {
  const entries = ["a", "b", "c"].map((id) => ({
    ...entryFromPath(`/music/${id}.wav`),
    id,
  }));
  it("uses entry IDs for moving repeated paths, including the final position", () => {
    expect(moveTarget(entries, 0, -1)).toBeUndefined();
    expect(moveTarget(entries, 1, -1)).toBe("a");
    expect(moveTarget(entries, 0, 1)).toBe("c");
    expect(moveTarget(entries, 1, 1)).toBeNull();
    expect(moveTarget(entries, 2, 1)).toBeUndefined();
    const duplicates: MediaEntry[] = [
      { ...entries[0], id: "one" },
      { ...entries[0], id: "two" },
    ];
    expect(moveTarget(duplicates, 1, -1)).toBe("one");
  });
  it("retains path, stable library identity, seconds-to-ms duration and missing state", () => {
    const t = {
      id: "local-1",
      local_path: "/music/a.wav",
      title: "Title",
      format: "Wav",
      duration: 123,
      missing: true,
    } as LibraryTrack;
    expect(entryFromTrack(t)).toMatchObject({
      trackId: "local-1",
      path: "/music/a.wav",
      title: "Title",
      durationMs: 123000,
      available: false,
    });
    expect(entryFromPath("C:\\Music\\a.flac").title).toBe("a.flac");
  });
});
