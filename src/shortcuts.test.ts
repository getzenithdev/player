import { describe, expect, it } from "vitest";
import { resolveShortcut, type ShortcutEvent } from "./shortcuts";

const base: ShortcutEvent = {
  key: " ",
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
};

describe("resolveShortcut", () => {
  it("toggles playback with Space and seeks with the arrow keys", () => {
    expect(resolveShortcut(base)).toBe("toggle");
    expect(resolveShortcut({ ...base, key: "ArrowRight" })).toBe(
      "seek-forward",
    );
    expect(resolveShortcut({ ...base, key: "ArrowLeft" })).toBe(
      "seek-backward",
    );
  });

  it("uses the primary modifier for track changes and muting", () => {
    expect(resolveShortcut({ ...base, key: "ArrowRight", ctrlKey: true })).toBe(
      "next",
    );
    expect(resolveShortcut({ ...base, key: "ArrowLeft", metaKey: true })).toBe(
      "previous",
    );
    expect(resolveShortcut({ ...base, key: "m", ctrlKey: true })).toBe("mute");
    expect(resolveShortcut({ ...base, key: "m" })).toBe("mute");
  });

  it("keeps editing and activation keys inside focused controls", () => {
    expect(
      resolveShortcut({ ...base, target: { tagName: "INPUT" } }),
    ).toBeNull();
    expect(
      resolveShortcut({ ...base, target: { tagName: "BUTTON" } }),
    ).toBeNull();
    expect(
      resolveShortcut({
        ...base,
        target: { tagName: "DIV", isContentEditable: true },
      }),
    ).toBeNull();
  });

  it("ignores repeats, handled events and modifier-only combinations", () => {
    expect(resolveShortcut({ ...base, repeat: true })).toBeNull();
    expect(resolveShortcut({ ...base, defaultPrevented: true })).toBeNull();
    expect(resolveShortcut({ ...base, shiftKey: true })).toBeNull();
  });
});
