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
    external: false,
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
