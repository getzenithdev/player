import { invoke, isTauri } from "@tauri-apps/api/core";

/**
 * Centralized preferences (roadmap phase 13).
 *
 * `config/settings.json` on the Rust side owns these values and mirrors the
 * theme, lyric and artwork sections into the documents used by earlier
 * modules. The interface applies appearance options to the document root as
 * data attributes and CSS variables, so they survive route changes.
 */

export type Accent = "violet" | "plum" | "rose" | "teal" | "amber" | "white";
export type AnimationMode = "full" | "reduced";
export type BackgroundMode = "dynamic" | "solid";
export type PlayerLayout = "full" | "compact";
export type LyricsStyle = "balanced" | "focus" | "lyrics";
export type RepeatMode = "off" | "all" | "one";
export type LyricProvider = "local" | "embedded" | "lrclib";
export type LyricFont = "system" | "serif" | "sans-serif";
export type ArtworkProvider =
  "local" | "apple-music" | "archive-org" | "youtube";
export type ArtworkQuality = "thumbnail" | "preview";
export type Language = "es" | "system";

export interface GeneralSettings {
  language: Language;
  confirmDestructive: boolean;
}

export interface AppearanceSettings {
  theme: string;
  accent: Accent;
  scale: number;
  animations: AnimationMode;
  background: BackgroundMode;
  playerLayout: PlayerLayout;
  lyricsStyle: LyricsStyle;
}

export interface PlaybackSettings {
  initialVolume: number;
  shuffle: boolean;
  repeat: RepeatMode;
}

export interface LyricsSettings {
  providers: LyricProvider[];
  size: number;
  font: LyricFont;
  color: string;
  activeColor: string;
  offsetMs: number;
}

export interface ArtworkSettings {
  providerPriority: ArtworkProvider[];
  animate: boolean;
  loopAnimation: boolean;
  batterySaver: boolean;
  quality: ArtworkQuality;
  allowDownloads: boolean;
}

export interface AppSettings {
  general: GeneralSettings;
  appearance: AppearanceSettings;
  playback: PlaybackSettings;
  lyrics: LyricsSettings;
  artwork: ArtworkSettings;
}

export interface SettingsSnapshot {
  settings: AppSettings;
  document: string;
  stored: boolean;
}

export const defaultSettings: AppSettings = {
  general: { language: "es", confirmDestructive: true },
  appearance: {
    theme: "violet",
    accent: "violet",
    scale: 100,
    animations: "full",
    background: "dynamic",
    playerLayout: "full",
    lyricsStyle: "balanced",
  },
  playback: { initialVolume: 70, shuffle: false, repeat: "off" },
  lyrics: {
    providers: ["local", "embedded", "lrclib"],
    size: 30,
    font: "system",
    color: "#777777",
    activeColor: "#8956dd",
    offsetMs: 0,
  },
  artwork: {
    providerPriority: ["local", "apple-music", "archive-org", "youtube"],
    animate: false,
    loopAnimation: true,
    batterySaver: true,
    quality: "preview",
    allowDownloads: false,
  },
};

export const themes = [
  { value: "dark", label: "Oscuro" },
  { value: "black", label: "Negro" },
  { value: "light", label: "Claro" },
  { value: "white", label: "Blanco" },
  { value: "violet", label: "Violeta" },
  { value: "purple", label: "Morado" },
  { value: "system", label: "Automático" },
];

export const accents: { value: Accent; label: string; color: string }[] = [
  { value: "violet", label: "Violeta", color: "#d0afe5" },
  { value: "plum", label: "Ciruela", color: "#c48fd6" },
  { value: "rose", label: "Rosa", color: "#eda6bc" },
  { value: "teal", label: "Verde azulado", color: "#8fd6cf" },
  { value: "amber", label: "Ámbar", color: "#e9c48c" },
  { value: "white", label: "Blanco", color: "#ffffff" },
];

export const lyricProviders: {
  value: LyricProvider;
  label: string;
  local: boolean;
}[] = [
  { value: "local", label: "Archivos .lrc locales", local: true },
  { value: "embedded", label: "Metadatos de la canción", local: true },
  { value: "lrclib", label: "LRCLIB (requiere conexión)", local: false },
];

export const artworkProviders: { value: ArtworkProvider; label: string }[] = [
  { value: "local", label: "Archivos locales" },
  { value: "apple-music", label: "Apple Music" },
  { value: "archive-org", label: "Archive.org" },
  { value: "youtube", label: "YouTube" },
];

const LANGUAGES: Language[] = ["es", "system"];
const ACCENTS: Accent[] = ["violet", "plum", "rose", "teal", "amber", "white"];
const ANIMATIONS: AnimationMode[] = ["full", "reduced"];
const BACKGROUNDS: BackgroundMode[] = ["dynamic", "solid"];
const PLAYER_LAYOUTS: PlayerLayout[] = ["full", "compact"];
const LYRICS_STYLES: LyricsStyle[] = ["balanced", "focus", "lyrics"];
const REPEAT_MODES: RepeatMode[] = ["off", "all", "one"];
const LYRIC_PROVIDERS: LyricProvider[] = ["local", "embedded", "lrclib"];
const LYRIC_FONTS: LyricFont[] = ["system", "serif", "sans-serif"];
const ARTWORK_PROVIDERS: ArtworkProvider[] = [
  "local",
  "apple-music",
  "archive-org",
  "youtube",
];
const ARTWORK_QUALITIES: ArtworkQuality[] = ["thumbnail", "preview"];

const MIN_SCALE = 75;
const MAX_SCALE = 200;
const MIN_LYRIC_SIZE = 16;
const MAX_LYRIC_SIZE = 80;
const MIN_LYRIC_OFFSET = -600_000;
const MAX_LYRIC_OFFSET = 600_000;

function clamp(
  value: number,
  min: number,
  max: number,
  fallback: number,
): number {
  return Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

function oneOf<T extends string>(
  value: string,
  allowed: readonly T[],
  fallback: T,
): T {
  return (allowed as readonly string[]).includes(value)
    ? (value as T)
    : fallback;
}

function uniqueAllowed<T extends string>(
  values: readonly string[],
  allowed: readonly T[],
): T[] {
  const result: T[] = [];
  for (const value of values) {
    if (
      (allowed as readonly string[]).includes(value) &&
      !result.includes(value as T)
    )
      result.push(value as T);
  }
  return result;
}

function isHexColor(value: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(value);
}

/** Mirrors `AppSettings::normalized` on the Rust side. */
export function normalizeSettings(settings: AppSettings): AppSettings {
  const general = settings.general ?? defaultSettings.general;
  const appearance = settings.appearance ?? defaultSettings.appearance;
  const playback = settings.playback ?? defaultSettings.playback;
  const lyrics = settings.lyrics ?? defaultSettings.lyrics;
  const artwork = settings.artwork ?? defaultSettings.artwork;

  const providers = uniqueAllowed(lyrics.providers ?? [], LYRIC_PROVIDERS);
  const priority = uniqueAllowed(
    artwork.providerPriority ?? [],
    ARTWORK_PROVIDERS,
  );
  for (const provider of ARTWORK_PROVIDERS) {
    if (!priority.includes(provider)) priority.push(provider);
  }

  return {
    general: {
      language: oneOf(general.language, LANGUAGES, "es"),
      confirmDestructive: Boolean(general.confirmDestructive),
    },
    appearance: {
      theme: oneOf(
        appearance.theme,
        themes.map((theme) => theme.value),
        "violet",
      ),
      accent: oneOf(appearance.accent, ACCENTS, "violet"),
      scale: Math.round(clamp(appearance.scale, MIN_SCALE, MAX_SCALE, 100)),
      animations: oneOf(appearance.animations, ANIMATIONS, "full"),
      background: oneOf(appearance.background, BACKGROUNDS, "dynamic"),
      playerLayout: oneOf(appearance.playerLayout, PLAYER_LAYOUTS, "full"),
      lyricsStyle: oneOf(appearance.lyricsStyle, LYRICS_STYLES, "balanced"),
    },
    playback: {
      initialVolume: Math.round(clamp(playback.initialVolume, 0, 100, 70)),
      shuffle: Boolean(playback.shuffle),
      repeat: oneOf(playback.repeat, REPEAT_MODES, "off"),
    },
    lyrics: {
      providers: providers.length
        ? providers
        : [...defaultSettings.lyrics.providers],
      size: Math.round(clamp(lyrics.size, MIN_LYRIC_SIZE, MAX_LYRIC_SIZE, 30)),
      font: oneOf(lyrics.font, LYRIC_FONTS, "system"),
      color: isHexColor(lyrics.color)
        ? lyrics.color
        : defaultSettings.lyrics.color,
      activeColor: isHexColor(lyrics.activeColor)
        ? lyrics.activeColor
        : defaultSettings.lyrics.activeColor,
      offsetMs: Math.round(
        clamp(lyrics.offsetMs, MIN_LYRIC_OFFSET, MAX_LYRIC_OFFSET, 0),
      ),
    },
    artwork: {
      providerPriority: priority,
      animate: Boolean(artwork.animate),
      loopAnimation: Boolean(artwork.loopAnimation),
      batterySaver: Boolean(artwork.batterySaver),
      quality: oneOf(artwork.quality, ARTWORK_QUALITIES, "preview"),
      allowDownloads: Boolean(artwork.allowDownloads),
    },
  };
}

/**
 * Applies appearance options globally. Theme stays owned by `App`, which
 * already loads the mirrored `appearance.json` document.
 */
export function applyPreferences(settings: AppSettings): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const appearance = settings.appearance;
  root.dataset.accent = appearance.accent;
  root.dataset.animations = appearance.animations;
  root.dataset.background = appearance.background;
  root.dataset.playerLayout = appearance.playerLayout;
  root.dataset.lyricsStyle = appearance.lyricsStyle;
  root.dataset.uiScale = String(appearance.scale);
  root.style.setProperty("--ui-scale", String(appearance.scale / 100));
  root.style.setProperty("--ui-inverse-scale", String(100 / appearance.scale));
}

/** Loads and applies settings before the first render settles. */
export async function bootstrapPreferences(): Promise<AppSettings> {
  let snapshot: SettingsSnapshot | null = null;
  try {
    snapshot = await loadSettings();
  } catch {
    /* A missing desktop backend keeps the documented defaults. */
  }
  const settings = snapshot?.settings ?? defaultSettings;
  applyPreferences(settings);
  try {
    await applyPlaybackDefaults(settings);
  } catch {
    /* Playback defaults never block startup. */
  }
  return settings;
}

/**
 * Playback defaults only cover a first start or an empty queue: a restored
 * session already owns its volume, shuffle and repeat state.
 */
async function applyPlaybackDefaults(settings: AppSettings): Promise<void> {
  if (!isTauri()) return;
  const state = await invoke<{ queue: unknown[] }>("audio_get_state");
  if (Array.isArray(state.queue) && state.queue.length > 0) return;
  await invoke("audio_set_volume", {
    volume: settings.playback.initialVolume / 100,
  });
  await invoke("audio_set_shuffle", {
    shuffle: settings.playback.shuffle,
  });
  await invoke("audio_set_repeat", { mode: settings.playback.repeat });
}

function nativeRuntime(): boolean {
  return isTauri();
}

/** Stored preferences, or `null` in a plain browser. */
export async function loadSettings(): Promise<SettingsSnapshot | null> {
  if (!nativeRuntime()) return null;
  const snapshot = await invoke<SettingsSnapshot>("settings_get");
  return { ...snapshot, settings: normalizeSettings(snapshot.settings) };
}

/** Validates, stores and mirrors the preferences. */
export async function saveSettings(
  settings: AppSettings,
): Promise<SettingsSnapshot | null> {
  if (!nativeRuntime()) return null;
  const snapshot = await invoke<SettingsSnapshot>("settings_save", {
    settings: normalizeSettings(settings),
  });
  return { ...snapshot, settings: normalizeSettings(snapshot.settings) };
}

/** Restores the documented defaults in the desktop application. */
export async function resetSettings(): Promise<SettingsSnapshot | null> {
  if (!nativeRuntime()) return null;
  const snapshot = await invoke<SettingsSnapshot>("settings_reset");
  return { ...snapshot, settings: normalizeSettings(snapshot.settings) };
}
