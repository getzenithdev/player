import { expect, it } from "vitest";
import { emptyArtwork, safeSourceUrl, shouldAnimate } from "./artwork";
it("requires every animation condition and disables it for reduced motion or battery saving", () => {
  const p = {
    ...emptyArtwork.preferences,
    autoplay: true,
    battery_saver: false,
  };
  expect(shouldAnimate(p, false, true, true, false)).toBe(true);
  expect(shouldAnimate(p, true, true, true, false)).toBe(false);
  expect(shouldAnimate(p, false, false, true, false)).toBe(false);
  expect(shouldAnimate(p, false, true, false, false)).toBe(false);
  expect(shouldAnimate(p, false, true, true, true)).toBe(false);
  expect(
    shouldAnimate({ ...p, battery_saver: true }, false, true, true, false),
  ).toBe(false);
});
it("only renders HTTPS provenance without credentials", () => {
  expect(safeSourceUrl("javascript:alert(1)")).toBeUndefined();
  expect(safeSourceUrl("https://user:secret@example.com")).toBeUndefined();
  expect(safeSourceUrl(null)).toBeUndefined();
  expect(safeSourceUrl("https://archive.org/details/album")).toBe(
    "https://archive.org/details/album",
  );
});
