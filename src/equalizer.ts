import { invoke, isTauri } from "@tauri-apps/api/core";

export const EQ_BANDS = [
  { hz: 31, label: "31" },
  { hz: 62, label: "62" },
  { hz: 125, label: "125" },
  { hz: 250, label: "250" },
  { hz: 500, label: "500" },
  { hz: 1000, label: "1k" },
  { hz: 2000, label: "2k" },
  { hz: 4000, label: "4k" },
  { hz: 8000, label: "8k" },
  { hz: 16000, label: "16k" },
] as const;

export const MIN_GAIN_DB = -12;
export const MAX_GAIN_DB = 12;
export const MAX_CROSSFADE_MS = 12_000;
export const MAX_CUSTOM_PRESETS = 40;

export type ReplayGainMode = "off" | "track" | "album";

export interface CustomPreset {
  name: string;
  gains: number[];
  preampDb: number;
}

export interface EqualizerSettings {
  enabled: boolean;
  preset: string;
  gains: number[];
  preampDb: number;
  balance: number;
  replayGain: ReplayGainMode;
  normalization: boolean;
  crossfadeMs: number;
  clipGuard: boolean;
  customPresets: CustomPreset[];
}

export interface ProcessingDiagnostics {
  sampleRate: number | null;
  supportedBands: boolean[];
  activeBands: number;
  replayGainDb: number;
  preampDb: number;
  headroomDb: number;
  effectiveGainDb: number;
  limiterEngaged: boolean;
}

export interface ProcessingSnapshot {
  settings: EqualizerSettings;
  diagnostics: ProcessingDiagnostics;
}

export interface BuiltinPreset {
  id: string;
  label: string;
  gains: number[];
  preampDb: number;
}

/** Mirrors `BUILTIN_PRESETS` on the Rust side. */
export const EQ_PRESETS: BuiltinPreset[] = [
  {
    id: "flat",
    label: "Plano",
    gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    preampDb: 0,
  },
  {
    id: "rock",
    label: "Rock",
    gains: [5, 4, 3, 1, -1, -2, -1, 1, 3, 4],
    preampDb: 0,
  },
  {
    id: "pop",
    label: "Pop",
    gains: [-1, 0, 1, 2, 3, 2, 0, -1, -1, -1],
    preampDb: 0,
  },
  {
    id: "classical",
    label: "Clásica",
    gains: [4, 3, 2, 1, 0, 0, -1, -2, -3, -4],
    preampDb: 0,
  },
  {
    id: "jazz",
    label: "Jazz",
    gains: [3, 2, 1, 2, -1, -1, 0, 1, 2, 3],
    preampDb: 0,
  },
  {
    id: "electronic",
    label: "Electrónica",
    gains: [5, 4, 1, 0, -2, 2, 1, 3, 4, 5],
    preampDb: 0,
  },
  {
    id: "hip-hop",
    label: "Hip-Hop",
    gains: [6, 5, 3, 1, -1, -1, 1, 2, 3, 4],
    preampDb: 0,
  },
  {
    id: "bass-boost",
    label: "Graves reforzados",
    gains: [8, 7, 5, 3, 1, 0, 0, 0, 0, 0],
    preampDb: 0,
  },
  {
    id: "vocal",
    label: "Voz",
    gains: [-2, -1, 1, 3, 5, 5, 3, 1, 0, -1],
    preampDb: 0,
  },
  {
    id: "acoustic",
    label: "Acústica",
    gains: [3, 2, 1, 1, 0, 1, 2, 2, 1, 0],
    preampDb: 0,
  },
];

export const defaultEqualizerSettings: EqualizerSettings = {
  enabled: false,
  preset: "flat",
  gains: Array.from({ length: EQ_BANDS.length }, () => 0),
  preampDb: 0,
  balance: 0,
  replayGain: "off",
  normalization: false,
  crossfadeMs: 0,
  clipGuard: true,
  customPresets: [],
};

export const defaultDiagnostics: ProcessingDiagnostics = {
  sampleRate: null,
  supportedBands: Array.from({ length: EQ_BANDS.length }, () => true),
  activeBands: 0,
  replayGainDb: 0,
  preampDb: 0,
  headroomDb: 0,
  effectiveGainDb: 0,
  limiterEngaged: false,
};

export const defaultProcessing: ProcessingSnapshot = {
  settings: defaultEqualizerSettings,
  diagnostics: defaultDiagnostics,
};

function clamp(value: number, min: number, max: number, fallback: number) {
  return Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

/** Mirrors `EqualizerSettings::normalized` on the Rust side. */
export function normalizeEqualizerSettings(
  settings: EqualizerSettings,
): EqualizerSettings {
  const seen = new Set<string>();
  const customPresets: CustomPreset[] = [];
  for (const preset of settings.customPresets) {
    const name = preset.name.trim().slice(0, 60);
    if (!name || seen.has(name)) continue;
    seen.add(name);
    customPresets.push({
      name,
      gains: EQ_BANDS.map((_, index) =>
        clamp(preset.gains[index] ?? 0, MIN_GAIN_DB, MAX_GAIN_DB, 0),
      ),
      preampDb: clamp(preset.preampDb, MIN_GAIN_DB, MAX_GAIN_DB, 0),
    });
    if (customPresets.length === MAX_CUSTOM_PRESETS) break;
  }

  const replayGain: ReplayGainMode = ["track", "album"].includes(
    settings.replayGain,
  )
    ? settings.replayGain
    : "off";
  return {
    enabled: Boolean(settings.enabled),
    preset: (settings.preset || "flat").trim().slice(0, 80) || "flat",
    gains: EQ_BANDS.map((_, index) =>
      clamp(settings.gains[index] ?? 0, MIN_GAIN_DB, MAX_GAIN_DB, 0),
    ),
    preampDb: clamp(settings.preampDb, MIN_GAIN_DB, MAX_GAIN_DB, 0),
    balance: clamp(settings.balance, -1, 1, 0),
    replayGain,
    normalization: Boolean(settings.normalization),
    crossfadeMs: Math.round(
      clamp(settings.crossfadeMs, 0, MAX_CROSSFADE_MS, 0),
    ),
    clipGuard: Boolean(settings.clipGuard),
    customPresets,
  };
}

export function supportsBand(hz: number, sampleRate: number | null): boolean {
  if (!sampleRate) return true;
  return hz < sampleRate * 0.45;
}

export function presetMatches(
  settings: EqualizerSettings,
  preset: BuiltinPreset | CustomPreset,
): boolean {
  return (
    settings.gains.length === preset.gains.length &&
    settings.gains.every(
      (gain, index) => Math.abs(gain - preset.gains[index]) < 0.05,
    ) &&
    Math.abs(settings.preampDb - preset.preampDb) < 0.05
  );
}

/** Returns the matching preset id, or `custom` when nothing matches. */
export function detectPreset(settings: EqualizerSettings): string {
  for (const preset of EQ_PRESETS) {
    if (presetMatches(settings, preset)) return preset.id;
  }
  for (const preset of settings.customPresets) {
    if (presetMatches(settings, preset)) return `custom:${preset.name}`;
  }
  return "custom";
}

export function applyPreset(
  settings: EqualizerSettings,
  id: string,
): EqualizerSettings {
  const normalized = normalizeEqualizerSettings(settings);
  const builtin = EQ_PRESETS.find((preset) => preset.id === id);
  if (builtin) {
    return normalizeEqualizerSettings({
      ...normalized,
      preset: builtin.id,
      gains: [...builtin.gains],
      preampDb: builtin.preampDb,
    });
  }
  if (id.startsWith("custom:")) {
    const name = id.slice("custom:".length);
    const preset = normalized.customPresets.find(
      (candidate) => candidate.name === name,
    );
    if (preset) {
      return normalizeEqualizerSettings({
        ...normalized,
        preset: id,
        gains: [...preset.gains],
        preampDb: preset.preampDb,
      });
    }
  }
  return normalized;
}

/** Applies one band and marks the result as an unsaved custom curve. */
export function withBandGain(
  settings: EqualizerSettings,
  index: number,
  gain: number,
): EqualizerSettings {
  const normalized = normalizeEqualizerSettings(settings);
  normalized.gains[index] = clamp(gain, MIN_GAIN_DB, MAX_GAIN_DB, 0);
  normalized.preset = detectPreset(normalized);
  return normalized;
}

/** Stores the current curve under `name`, replacing a same-name preset. */
export function saveCustomPreset(
  settings: EqualizerSettings,
  name: string,
): EqualizerSettings {
  const cleaned = name.trim().slice(0, 60);
  if (!cleaned) throw new Error("Escribe un nombre para el preset.");
  const normalized = normalizeEqualizerSettings(settings);
  const preset: CustomPreset = {
    name: cleaned,
    gains: [...normalized.gains],
    preampDb: normalized.preampDb,
  };
  const index = normalized.customPresets.findIndex(
    (candidate) => candidate.name === cleaned,
  );
  if (index >= 0) normalized.customPresets[index] = preset;
  else {
    if (normalized.customPresets.length >= MAX_CUSTOM_PRESETS)
      throw new Error(
        `Solo se pueden guardar ${MAX_CUSTOM_PRESETS} presets personalizados.`,
      );
    normalized.customPresets.push(preset);
  }
  normalized.preset = `custom:${cleaned}`;
  return normalized;
}

export function removeCustomPreset(
  settings: EqualizerSettings,
  name: string,
): EqualizerSettings {
  const normalized = normalizeEqualizerSettings(settings);
  normalized.customPresets = normalized.customPresets.filter(
    (preset) => preset.name !== name,
  );
  if (normalized.preset === `custom:${name}`) normalized.preset = "custom";
  return normalized;
}

export function formatGain(value: number): string {
  if (Math.abs(value) < 0.05) return "0 dB";
  return `${value > 0 ? "+" : ""}${value.toFixed(1)} dB`;
}

function nativeRuntime(): boolean {
  return isTauri();
}

export async function loadProcessing(): Promise<ProcessingSnapshot | null> {
  if (!nativeRuntime()) return null;
  return invoke<ProcessingSnapshot>("audio_processing_get");
}

export async function saveProcessing(
  settings: EqualizerSettings,
): Promise<ProcessingSnapshot | null> {
  if (!nativeRuntime()) return null;
  return invoke<ProcessingSnapshot>("audio_processing_save", {
    settings: normalizeEqualizerSettings(settings),
  });
}
