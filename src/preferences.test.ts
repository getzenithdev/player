import { describe, expect, it } from "vitest";
import {
  applyPreferences,
  bootstrapPreferences,
  defaultSettings,
  loadSettings,
  normalizeSettings,
  resetSettings,
  saveSettings,
  type AppSettings,
} from "./preferences";

describe("central preferences", () => {
  it("keeps the documented defaults valid", () => {
    const settings = normalizeSettings(defaultSettings);
    expect(settings).toEqual(defaultSettings);
    expect(settings.general.confirmDestructive).toBe(true);
    expect(settings.playback.initialVolume).toBe(70);
    expect(settings.lyrics.providers).toEqual(["local", "embedded", "lrclib"]);
    expect(settings.artwork.allowDownloads).toBe(false);
  });

  it("clamps ranges and repairs enum values", () => {
    const settings = normalizeSettings({
      ...defaultSettings,
      general: {
        ...defaultSettings.general,
        language: "fr" as AppSettings["general"]["language"],
      },
      appearance: {
        ...defaultSettings.appearance,
        theme: "neon",
        accent: "neon" as AppSettings["appearance"]["accent"],
        scale: 500,
      },
      playback: {
        ...defaultSettings.playback,
        initialVolume: 300,
        repeat: "loop" as AppSettings["playback"]["repeat"],
      },
      lyrics: {
        ...defaultSettings.lyrics,
        size: 2,
        offsetMs: 900_000,
        color: "red",
      },
      artwork: {
        ...defaultSettings.artwork,
        quality: "ultra" as AppSettings["artwork"]["quality"],
      },
    });
    expect(settings.general.language).toBe("es");
    expect(settings.appearance.accent).toBe("violet");
    expect(settings.appearance.theme).toBe("light");
    expect(settings.appearance.scale).toBe(200);
    expect(settings.playback.initialVolume).toBe(100);
    expect(settings.playback.repeat).toBe("off");
    expect(settings.lyrics.size).toBe(16);
    expect(settings.lyrics.offsetMs).toBe(600_000);
    expect(settings.lyrics.color).toBe("#777777");
    expect(settings.artwork.quality).toBe("preview");
  });

  it("deduplicates providers and completes the artwork priority", () => {
    const settings = normalizeSettings({
      ...defaultSettings,
      lyrics: {
        ...defaultSettings.lyrics,
        providers: [
          "lrclib",
          "lrclib",
          "unknown" as AppSettings["lyrics"]["providers"][number],
          "local",
        ],
      },
      artwork: {
        ...defaultSettings.artwork,
        providerPriority: [
          "youtube",
          "youtube",
        ] as AppSettings["artwork"]["providerPriority"],
      },
    });
    expect(settings.lyrics.providers).toEqual(["lrclib", "local"]);
    expect(settings.artwork.providerPriority).toEqual([
      "youtube",
      "local",
      "apple-music",
      "archive-org",
    ]);
  });

  it("never leaves an empty lyric provider list", () => {
    const settings = normalizeSettings({
      ...defaultSettings,
      lyrics: { ...defaultSettings.lyrics, providers: [] },
    });
    expect(settings.lyrics.providers).toEqual(["local", "embedded", "lrclib"]);
  });

  it("applies appearance options without a document in tests", () => {
    expect(() => applyPreferences(defaultSettings)).not.toThrow();
  });

  it("returns defaults outside the desktop runtime", async () => {
    await expect(loadSettings()).resolves.toBeNull();
    await expect(saveSettings(defaultSettings)).resolves.toBeNull();
    await expect(resetSettings()).resolves.toBeNull();
    await expect(bootstrapPreferences()).resolves.toEqual(defaultSettings);
  });
});
