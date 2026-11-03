import type { PlaybackStatus } from "./audio";

export interface AnnouncementTrack {
  title: string;
  artist?: string | null;
  album?: string | null;
}

/** Text announced to screen readers when playback changes. */
export function playbackAnnouncement(
  track: AnnouncementTrack | null,
  status: PlaybackStatus,
): string {
  const labels: Record<PlaybackStatus, string> = {
    idle: "Reproducción detenida",
    loading: "Cargando pista",
    playing: "Reproduciendo",
    paused: "En pausa",
    stopped: "Reproducción detenida",
    error: "Error de reproducción",
  };
  const base = labels[status];
  if (!track) return base;
  const by = [track.artist, track.album].filter(Boolean).join(", ");
  return by ? `${base}: ${track.title}, ${by}` : `${base}: ${track.title}`;
}

/** Data attributes that force the visual accessibility overrides. */
export function accessibilityAttributes(preferences: {
  reducedMotion: boolean;
  highContrast: boolean;
}): Record<string, string> {
  const attributes: Record<string, string> = {};
  if (preferences.reducedMotion) attributes["data-reduced-motion"] = "true";
  if (preferences.highContrast) attributes["data-high-contrast"] = "true";
  return attributes;
}
