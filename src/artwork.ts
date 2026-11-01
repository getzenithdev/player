import { invoke } from "@tauri-apps/api/core";
export interface ArtworkAsset {
  id: string;
  provider: string;
  title: string;
  artist: string;
  album: string;
  source_url: string | null;
  license: string;
  attribution: string;
  saved_at: number;
  width: number;
  height: number;
  animated: boolean;
  score: number;
}
export interface ArtworkPreferences {
  external: boolean;
  autoplay: boolean;
  looping: boolean;
  battery_saver: boolean;
  quality: "thumbnail" | "preview";
}
export interface ArtworkSnapshot {
  resolved: ArtworkAsset | null;
  candidates: ArtworkAsset[];
  preferences: ArtworkPreferences;
  warnings: string[];
}
export const emptyArtwork: ArtworkSnapshot = {
  resolved: null,
  candidates: [],
  preferences: {
    external: false,
    autoplay: false,
    looping: true,
    battery_saver: true,
    quality: "preview",
  },
  warnings: [],
};
export const artworkAction = (
  trackId: string,
  action: object,
): Promise<ArtworkSnapshot> => invoke("artwork_action", { trackId, action });
export function shouldAnimate(
  { autoplay, battery_saver }: ArtworkPreferences,
  reduced: boolean,
  visible: boolean,
  pageVisible: boolean,
  paused: boolean,
) {
  return (
    autoplay && !battery_saver && !reduced && visible && pageVisible && !paused
  );
}
export function safeSourceUrl(value: string | null): string | undefined {
  try {
    const u = new URL(value ?? "");
    if (u.protocol === "https:" && !u.username && !u.password)
      return value ?? undefined;
  } catch {
    /* Only provenance links, never arbitrary schemes. */
  }
  return undefined;
}
