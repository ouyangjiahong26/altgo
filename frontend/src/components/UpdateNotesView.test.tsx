import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import UpdateNotesView from "./UpdateNotesView";
import { RELEASES_URL, type UpdateInstallState } from "../updateNotes";

const openUrlMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrlMock(url),
}));

// 外部引导档（rpm/AUR）的检查结果形态。
const externalInfo = {
  hasUpdate: true,
  currentVersion: "2.7.0",
  latestVersion: "2.7.1",
  supportTier: "external" as const,
  body: "### Fixes\n\n- probe",
};

// deb 直装档的检查结果形态。
const debInfo = { ...externalInfo, supportTier: "deb_package" as const };

const baseProps = {
  info: externalInfo,
  installState: "idle" as UpdateInstallState,
  progress: null,
  installError: null,
  onInstall: vi.fn(),
  onRestart: vi.fn(),
  onClose: vi.fn(),
};

beforeEach(() => {
  localStorage.setItem("altgo-lang", "en");
  openUrlMock.mockClear();
});

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

describe("UpdateNotesView deb 直装档", () => {
  it("点击“下载并安装”调用 onInstall", () => {
    const onInstall = vi.fn();
    render(
      <UpdateNotesView {...baseProps} info={debInfo} onInstall={onInstall} />
    );

    fireEvent.click(screen.getByText("Download & Install"));

    expect(onInstall).toHaveBeenCalledTimes(1);
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("下载中展示百分比与进度条，按钮不可再点", () => {
    render(
      <UpdateNotesView
        {...baseProps}
        info={debInfo}
        installState="downloading"
        progress={{ phase: "downloading", downloaded: 25, total: 100 }}
      />
    );

    expect(screen.getByText("Downloading package")).toBeTruthy();
    expect(screen.getByText("25%")).toBeTruthy();
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
      "25"
    );
    expect(
      (screen.getByText("Download & Install").closest("button") as HTMLButtonElement)
        .disabled
    ).toBe(true);
  });

  it("等待授权时提示系统会要密码", () => {
    render(
      <UpdateNotesView {...baseProps} info={debInfo} installState="installing" />
    );

    expect(
      screen.getByText(
        "Waiting for administrator authorization, the system will ask for your password"
      )
    ).toBeTruthy();
  });

  it("安装完成后提供重启按钮", () => {
    const onRestart = vi.fn();
    render(
      <UpdateNotesView
        {...baseProps}
        info={debInfo}
        installState="installed"
        onRestart={onRestart}
      />
    );

    expect(screen.getByText("Installed. Restart the app to apply")).toBeTruthy();
    fireEvent.click(screen.getByText("Restart App"));
    expect(onRestart).toHaveBeenCalledTimes(1);
  });
});
