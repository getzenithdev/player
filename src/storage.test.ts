import { describe, expect, it } from "vitest";
import {
  createBackup,
  exportData,
  formatBytes,
  hasNativeStorage,
  importData,
  loadAppearance,
  loadStorageStatus,
  nativeOnlyMessage,
  saveAppearance,
  searchLibrary,
} from "./storage";

describe("storage outside the desktop runtime", () => {
  it("reports that native storage is unavailable", () => {
    expect(hasNativeStorage()).toBe(false);
  });

  it("returns null instead of failing when loading data", async () => {
    await expect(loadStorageStatus()).resolves.toBeNull();
    await expect(loadAppearance()).resolves.toBeNull();
    await expect(searchLibrary("canción")).resolves.toEqual([]);
  });

  it("keeps the theme as a preview without claiming it was saved", async () => {
    await expect(saveAppearance("dark")).resolves.toBe(false);
  });

  it("explains why actions require the desktop application", async () => {
    await expect(createBackup()).rejects.toThrow(nativeOnlyMessage);
    await expect(exportData()).rejects.toThrow(nativeOnlyMessage);
    await expect(importData()).rejects.toThrow(nativeOnlyMessage);
  });
});

describe("formatBytes", () => {
  it("renders zero and invalid sizes as bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-5)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });

  it("scales to the closest unit", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5 MB");
  });
});
