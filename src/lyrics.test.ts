import { describe, expect, it } from "vitest";
import { activeLine, activeWord } from "./lyrics";
describe("lyrics synchronized to engine position", () => {
  const lines = [
    { at: 1000, text: "one", words: [] },
    { at: 2000, text: "two", words: [] },
    { at: null, text: "plain", words: [] },
  ];
  it("handles seeking backwards, intro and exact boundaries", () => {
    expect(activeLine(lines, 0)).toBe(-1);
    expect(activeLine(lines, 2000)).toBe(1);
    expect(activeLine(lines, 1500)).toBe(0);
  });
  it("applies positive and negative offsets without selecting untimed lines", () => {
    expect(activeLine(lines, 1800, 200)).toBe(1);
    expect(activeLine(lines, 2100, -200)).toBe(0);
    expect(activeLine(lines, 100000)).toBe(1);
  });
  it("highlights words using the same paused or seeked position", () => {
    const words = [
      { at: 1000, text: "a" },
      { at: 1500, text: "b" },
    ];
    expect(activeWord(words, 1400)).toBe(0);
    expect(activeWord(words, 1400, 100)).toBe(1);
    expect(activeWord(words, 0)).toBe(-1);
  });
});
