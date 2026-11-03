import { describe, expect, it } from "vitest";
import { accessibilityAttributes, playbackAnnouncement } from "./accessibility";

describe("playbackAnnouncement", () => {
  it("describes the track and its metadata", () => {
    expect(
      playbackAnnouncement(
        { title: "Nightfall", artist: "Aurora", album: "Signals" },
        "playing",
      ),
    ).toBe("Reproduciendo: Nightfall, Aurora, Signals");
  });

  it("falls back to the state when nothing is loaded", () => {
    expect(playbackAnnouncement(null, "stopped")).toBe("Reproducción detenida");
  });
});

describe("accessibilityAttributes", () => {
  it("only emits explicit overrides", () => {
    expect(
      accessibilityAttributes({ reducedMotion: true, highContrast: false }),
    ).toEqual({ "data-reduced-motion": "true" });
    expect(
      accessibilityAttributes({ reducedMotion: false, highContrast: true }),
    ).toEqual({ "data-high-contrast": "true" });
    expect(
      accessibilityAttributes({ reducedMotion: false, highContrast: false }),
    ).toEqual({});
  });
});
