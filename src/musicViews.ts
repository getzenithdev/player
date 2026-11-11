import { groupIdentity, metadataKey, type LibraryTrack } from "./library";

export function formatTime(milliseconds: number | null) {
  const seconds = Math.max(0, Math.floor((milliseconds ?? 0) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
export function albumGroups(tracks: LibraryTrack[]) {
  const groups = new Map<string, LibraryTrack[]>();
  for (const track of tracks) {
    const key = groupIdentity(track, "Álbumes");
    const items = groups.get(key);
    if (items) items.push(track);
    else groups.set(key, [track]);
  }
  return [...groups.values()];
}
export function searchMusic(tracks: LibraryTrack[], query: string) {
  const needle = metadataKey(query);
  return needle
    ? tracks.filter((t) =>
        [t.title, t.artist, t.album, t.genre].some((v) =>
          metadataKey(v ?? "").includes(needle),
        ),
      )
    : [];
}
export function playableTracks(tracks: LibraryTrack[]) {
  return tracks.filter(isPlayableTrack);
}
export function isPlayableTrack(track: LibraryTrack) {
  return (
    !track.missing &&
    (!!track.local_path ||
      (!!track.source_id &&
        [
          "youtube",
          "youtube-auto",
          "google-drive",
          "dropbox",
          "onedrive",
          "webdav",
        ].includes(track.source ?? "")))
  );
}
