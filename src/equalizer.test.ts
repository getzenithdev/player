import { describe, expect, it } from "vitest";
import {
  EQ_BANDS,
  EQ_PRESETS,
  MAX_CUSTOM_PRESETS,
  applyPreset,
  defaultEqualizerSettings,
  detectPreset,
  formatGain,
  normalizeEqualizerSettings,
  removeCustomPreset,
  saveCustomPreset,
  supportsBand,
  withBandGain,
  type EqualizerSettings,
} from "./equalizer";

describe("equalizer settings", () => {
  it("clamps ranges and removes invalid custom presets", () => {
    const normalized = normalizeEqualizerSettings({
      ...defaultEqualizerSettings,
      preset: "  ",
      gains: [20, -20, 1, 2, 3, 4, 5, 6, 7, 8],
      preampDb: 40,
      balance: -4,
      replayGain: "track",
      crossfadeMs: 99_999,
      customPresets: [
        { name: "  Voz  ", gains: Array(10).fill(20), preampDb: -20 },
        { name: "Voz", gains: Array(10).fill(0), preampDb: 0 },
        { name: "   ", gains: Array(10).fill(0), preampDb: 0 },
      ],
    });

    expect(normalized.gains).toEqual([12, -12, 1, 2, 3, 4, 5, 6, 7, 8]);
    expect(normalized.preampDb).toBe(12);
    expect(normalized.balance).toBe(-1);
    expect(normalized.crossfadeMs).toBe(12_000);
    expect(normalized.preset).toBe("flat");
    expect(normalized.customPresets).toHaveLength(1);
    expect(normalized.customPresets[0].name).toBe("Voz");
    expect(normalized.customPresets[0].gains[0]).toBe(12);
  });

  it("applies built-in presets and detects their curve", () => {
    const rock = applyPreset(defaultEqualizerSettings, "rock");
    expect(rock.preset).toBe("rock");
    expect(rock.gains).toEqual(EQ_PRESETS.find((p) => p.id === "rock")?.gains);
    expect(detectPreset(rock)).toBe("rock");
    expect(detectPreset(applyPreset(rock, "flat"))).toBe("flat");
  });

  it("keeps the raw custom curve while bands are edited", () => {
    const edited = withBandGain(defaultEqualizerSettings, 0, 7.5);
    expect(edited.gains[0]).toBe(7.5);
    expect(edited.preset).toBe("custom");
    expect(detectPreset(edited)).toBe("custom");
  });

  it("stores, replaces and removes personal presets", () => {
    const first = saveCustomPreset(
      withBandGain(defaultEqualizerSettings, 2, 4),
      " Sala ",
    );
    expect(first.preset).toBe("custom:Sala");
    expect(first.customPresets).toHaveLength(1);

    const replacement = saveCustomPreset(withBandGain(first, 2, -4), "Sala");
    expect(replacement.customPresets).toHaveLength(1);
    expect(replacement.customPresets[0].gains[2]).toBe(-4);

    const removed = removeCustomPreset(replacement, "Sala");
    expect(removed.customPresets).toHaveLength(0);
    expect(removed.preset).toBe("custom");
    expect(() => saveCustomPreset(first, "   ")).toThrow();
  });

  it("limits the number of personal presets", () => {
    let settings: EqualizerSettings = defaultEqualizerSettings;
    for (let index = 0; index < MAX_CUSTOM_PRESETS; index += 1)
      settings = saveCustomPreset(settings, `Preset ${index}`);
    expect(settings.customPresets).toHaveLength(MAX_CUSTOM_PRESETS);
    expect(() => saveCustomPreset(settings, "Extra")).toThrow();
  });

  it("disables bands above the Nyquist limit", () => {
    expect(supportsBand(16_000, 44_100)).toBe(true);
    expect(supportsBand(16_000, 32_000)).toBe(false);
    expect(supportsBand(31, 8_000)).toBe(true);
    expect(supportsBand(16_000, null)).toBe(true);
    expect(EQ_BANDS).toHaveLength(10);
  });

  it("formats gains with an explicit sign", () => {
    expect(formatGain(0)).toBe("0 dB");
    expect(formatGain(3.5)).toBe("+3.5 dB");
    expect(formatGain(-2)).toBe("-2.0 dB");
  });
});
