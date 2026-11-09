import { invoke } from "@tauri-apps/api/core";
export interface LyricWord {
  at: number;
  text: string;
}
export interface LyricLine {
  at: number | null;
  text: string;
  words: LyricWord[];
}
export interface LyricCandidate {
  id: string;
  provider: string;
  title: string;
  artist: string;
  album: string;
  duration: number | null;
  raw: string;
  source_url: string | null;
  score: number;
  parsed: { lines: LyricLine[]; offset: number; warnings: string[] };
}
export interface LyricsPreferences {
  mode: "normal" | "karaoke" | "minimal" | "fullscreen";
  offset: number;
  size: number;
  font: string;
  color: string;
  active_color: string;
  external: boolean;
}
export interface LyricsSnapshot {
  saved: {
    selected: LyricCandidate | null;
    rejected: string[];
    offset: number;
    suppress_local: boolean;
  };
  preferences: LyricsPreferences;
  candidates: LyricCandidate[];
  warnings: string[];
}
export const defaultLyrics: LyricsSnapshot = {
  saved: { selected: null, rejected: [], offset: 0, suppress_local: false },
  preferences: {
    mode: "normal",
    offset: 0,
    size: 30,
    font: "system",
    color: "#777777",
    active_color: "#8956dd",
    external: true,
  },
  candidates: [],
  warnings: [],
};
export function lyricsAction(
  trackId: string,
  action: object,
): Promise<LyricsSnapshot> {
  return invoke("lyrics_action", { trackId, action });
}

// Share discovery between the full player and the lyrics page. Cache only
// proposals: saved selections and preferences always come from a fresh snapshot.
type LyricsDiscovery = Pick<LyricsSnapshot, "candidates" | "warnings">;
const discoveries = new Map<string, {
  expires: number;
  request: Promise<LyricsDiscovery>;
}>();

export function discoverLyrics(trackId: string, metadata: string): Promise<LyricsDiscovery> {
  const key = JSON.stringify([trackId, metadata]);
  const cached = discoveries.get(key);
  if (cached && cached.expires > Date.now()) return cached.request;
  const request = lyricsAction(trackId, { kind: "search", query: null })
    .then(({ candidates, warnings }) => ({ candidates, warnings }))
    .catch((error: unknown) => {
      discoveries.delete(key);
      throw error;
    });
  discoveries.set(key, { expires: Date.now() + 60_000, request });
  if (discoveries.size > 100) {
    const oldest = discoveries.keys().next().value;
    if (oldest !== undefined) discoveries.delete(oldest);
  }
  return request;
}

export function displayedLyrics(snapshot: LyricsSnapshot): LyricCandidate | null {
  if (snapshot.saved.selected) return snapshot.saved.selected;
  if (snapshot.saved.suppress_local) return null;
  const candidates = snapshot.candidates.filter((c) => !snapshot.saved.rejected.includes(c.id));
  const local = candidates.find((c) => c.provider === "local" || c.provider === "embedded");
  if (local) return local;
  // 75 requires both an exact normalized title (45) and artist (30).
  // Preview matching results without persisting or replacing a user's choice.
  return candidates.filter((c) => c.provider === "lrclib" && c.score >= 75)
    .sort((a, b) => b.score - a.score ||
      Number(b.parsed.lines.some((line) => line.at !== null)) -
      Number(a.parsed.lines.some((line) => line.at !== null)))[0] ?? null;
}
// Positive offsets show a line earlier (LRC convention). Position is owned by
// the audio engine: seeking, pausing and repeat never start a separate timer.
export function activeLine(
  lines: LyricLine[],
  position: number,
  offset = 0,
): number {
  let active = -1;
  for (let i = 0; i < lines.length; i++) {
    const at = lines[i].at;
    if (at !== null && at <= position + offset) active = i;
  }
  return active;
}
export function activeWord(
  words: LyricWord[],
  position: number,
  offset = 0,
): number {
  let active = -1;
  words.forEach((word, i) => {
    if (word.at <= position + offset) active = i;
  });
  return active;
}
