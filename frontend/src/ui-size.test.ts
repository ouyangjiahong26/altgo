import { beforeEach, describe, expect, it, vi } from "vitest";

const windowApi = vi.hoisted(() => ({
  isMaximized: vi.fn(),
  setSize: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => windowApi,
  LogicalSize: class {
    width: number;
    height: number;

    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
  },
}));

import {
  applyFontSize,
  applyWindowSize,
  FONT_SIZE_KEY,
  getFontSizePref,
  getWindowSizePref,
  WINDOW_SIZE_KEY,
} from "./ui-size";

describe("ui-size", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.style.fontSize = "";
    windowApi.isMaximized.mockReset();
    windowApi.isMaximized.mockResolvedValue(false);
    windowApi.setSize.mockReset();
    windowApi.setSize.mockResolvedValue(undefined);
  });

  it("uses enlarged defaults when no preference is saved", () => {
    expect(getFontSizePref()).toBe("medium");
    expect(getWindowSizePref()).toBe("standard");
  });

  it("falls back to defaults for invalid saved preferences", () => {
    localStorage.setItem(FONT_SIZE_KEY, "huge");
    localStorage.setItem(WINDOW_SIZE_KEY, "fullscreen");

    expect(getFontSizePref()).toBe("medium");
    expect(getWindowSizePref()).toBe("standard");
  });

  it("applies the selected root font size", () => {
    applyFontSize("large");

    expect(document.documentElement.style.fontSize).toBe("19px");
  });

  it("resizes the main window to the selected preset", async () => {
    await applyWindowSize("large");

    expect(windowApi.setSize).toHaveBeenCalledWith(
      expect.objectContaining({ width: 480, height: 760 }),
    );
  });

  it("does not resize a maximized window", async () => {
    windowApi.isMaximized.mockResolvedValue(true);

    await applyWindowSize("compact");

    expect(windowApi.setSize).not.toHaveBeenCalled();
  });
});
