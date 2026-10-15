import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { usePolishRuntime, type PolishRuntimeStatus } from "./usePolishRuntime";

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  message: vi.fn().mockResolvedValue(undefined),
}));

const installedStatus: PolishRuntimeStatus = {
  supported: true,
  installed: true,
  running: true,
  build: "test",
  archiveSize: 1,
};

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "polish_runtime_status"
      ? Promise.resolve(installedStatus)
      : Promise.resolve(null),
  );
  listenMock.mockImplementation(
    (event: string, cb: (e: { payload: unknown }) => void) => {
      if (event === "polish-runtime-download-finished") {
        Promise.resolve().then(() => cb({ payload: { success: true } }));
      }
      return Promise.resolve(() => {});
    },
  );
});

describe("usePolishRuntime 重新下载", () => {
  it("已安装后重新下载先移除运行时再下载", async () => {
    const { result } = renderHook(() => usePolishRuntime({ t: (k) => k }));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    await act(async () => {
      await result.current.redownload();
    });

    const commands = invokeMock.mock.calls.map((call) => call[0]);
    const removeAt = commands.indexOf("remove_polish_runtime");
    const downloadAt = commands.indexOf("download_polish_runtime");
    expect(removeAt).toBeGreaterThanOrEqual(0);
    expect(downloadAt).toBeGreaterThan(removeAt);
  });

  it("移除失败时中止下载", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "polish_runtime_status") return Promise.resolve(installedStatus);
      if (cmd === "remove_polish_runtime") return Promise.reject("remove failed");
      return Promise.resolve(null);
    });
    const { result } = renderHook(() => usePolishRuntime({ t: (k) => k }));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    await act(async () => {
      await result.current.redownload();
    });

    const commands = invokeMock.mock.calls.map((call) => call[0]);
    expect(commands).toContain("remove_polish_runtime");
    expect(commands).not.toContain("download_polish_runtime");
  });
});
