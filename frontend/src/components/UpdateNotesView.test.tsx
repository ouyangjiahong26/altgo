import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import UpdateNotesView from "./UpdateNotesView";
import { RELEASES_URL } from "../updateNotes";

const openUrlMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrlMock(url),
}));

// 外部引导档（deb/rpm/AUR）的检查结果形态。
const externalInfo = {
  hasUpdate: true,
  currentVersion: "2.7.0",
  latestVersion: "2.7.1",
  supportTier: "external" as const,
  body: "### Fixes\n\n- probe",
};

const baseProps = {
  info: externalInfo,
  installing: false,
  installError: null,
  onInstall: vi.fn(),
  onClose: vi.fn(),
};

describe("UpdateNotesView 外部引导档", () => {
  it("点击“打开下载页面”经 opener 打开系统浏览器，不调用 window.open", () => {
    const windowOpen = vi.spyOn(window, "open").mockReturnValue(null);
    render(<UpdateNotesView {...baseProps} />);

    fireEvent.click(screen.getByText("Open Release Page"));

    // WebView 里 window.open 会被静默拦截（无 new-window handler），必须走 opener。
    expect(openUrlMock).toHaveBeenCalledWith(RELEASES_URL);
    expect(windowOpen).not.toHaveBeenCalled();
    windowOpen.mockRestore();
  });
});
