import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useGlossaryCandidates,
  type GlossaryCandidateInfo,
} from "./useGlossaryCandidates";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const t = (key: string): string => key;

const remoteConfig = {
  polishBackend: "remote",
  polishApiBaseUrl: "https://api.example.com/v1",
  polishModel: "model",
  hasPolisherApiKey: true,
  polishGlossary: ["地月空间"],
};

const localConfig = {
  ...remoteConfig,
  polishBackend: "local",
};

const candidate: GlossaryCandidateInfo = {
  term: "平动点",
  fragment: "平洞点",
};

function mockCommands(handlers: Record<string, () => unknown>) {
  invokeMock.mockImplementation((cmd: string) => {
    const handler = handlers[cmd];
    if (!handler) return Promise.resolve(null);
    try {
      return Promise.resolve(handler());
    } catch (error) {
      return Promise.reject(error);
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("useGlossaryCandidates", () => {
  it("打开面板时读取配置并请求候选", async () => {
    mockCommands({
      get_config: () => remoteConfig,
      suggest_glossary_terms: () => [candidate],
    });
    const { result } = renderHook(() => useGlossaryCandidates(t));

    act(() => result.current.open("e1"));

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.remoteReady).toBe(true);
    expect(result.current.glossaryLoaded).toBe(true);
    expect(result.current.glossaryCount).toBe(1);
    expect(result.current.candidates).toEqual([candidate]);
    expect(invokeMock).toHaveBeenCalledWith("suggest_glossary_terms", {
      historyId: "e1",
    });
  });

  it("本地后端不请求候选，但允许手动添加", async () => {
    mockCommands({
      get_config: () => localConfig,
      save_config: () => null,
    });
    const { result } = renderHook(() => useGlossaryCandidates(t));

    act(() => result.current.open("e1"));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.remoteReady).toBe(false);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "suggest_glossary_terms",
      expect.anything(),
    );

    act(() => result.current.addManual("平动点"));
    act(() => result.current.add());
    await waitFor(() => expect(result.current.addedCount).toBe(1));

    expect(invokeMock).toHaveBeenCalledWith("save_config", {
      patch: { polishGlossary: ["地月空间", "平动点"] },
    });
    expect(result.current.glossaryCount).toBe(2);
  });

  it("部分候选无法加入时保留能加入的词并标出原因", async () => {
    mockCommands({
      get_config: () => remoteConfig,
      suggest_glossary_terms: () => [
        { term: "地月空间", fragment: "地约空间" },
        candidate,
      ],
      save_config: () => null,
    });
    const { result } = renderHook(() => useGlossaryCandidates(t));

    act(() => result.current.open("e1"));
    await waitFor(() => expect(result.current.candidates).toHaveLength(2));

    act(() => {
      result.current.toggle("地月空间");
      result.current.toggle("平动点");
    });
    act(() => result.current.add());
    await waitFor(() => expect(result.current.addedCount).toBe(1));

    expect(invokeMock).toHaveBeenCalledWith("save_config", {
      patch: { polishGlossary: ["地月空间", "平动点"] },
    });
    expect(result.current.skipped).toEqual([
      { term: "地月空间", reason: "duplicate" },
    ]);
    expect(result.current.candidates.map((item) => item.term)).toEqual([
      "地月空间",
    ]);
  });

  it("保存失败时保留候选并展示错误", async () => {
    mockCommands({
      get_config: () => remoteConfig,
      suggest_glossary_terms: () => [candidate],
      save_config: () => Promise.reject("save failed"),
    });
    const { result } = renderHook(() => useGlossaryCandidates(t));

    act(() => result.current.open("e1"));
    await waitFor(() => expect(result.current.candidates).toHaveLength(1));

    act(() => result.current.toggle("平动点"));
    act(() => result.current.add());
    await waitFor(() => expect(result.current.error).toBe("save failed"));

    expect(result.current.addedCount).toBe(0);
    expect(result.current.candidates).toEqual([candidate]);
    expect(result.current.glossaryCount).toBe(1);
  });

  it("关闭面板后忽略仍在途的候选响应", async () => {
    let resolveCandidates: ((value: GlossaryCandidateInfo[]) => void) | null =
      null;
    mockCommands({
      get_config: () => remoteConfig,
      suggest_glossary_terms: () =>
        new Promise<GlossaryCandidateInfo[]>((resolve) => {
          resolveCandidates = resolve;
        }),
    });
    const { result } = renderHook(() => useGlossaryCandidates(t));

    act(() => result.current.open("e1"));
    await waitFor(() => expect(resolveCandidates).not.toBeNull());

    act(() => result.current.close());
    await act(async () => {
      resolveCandidates?.([candidate]);
    });

    expect(result.current.openId).toBeNull();
    expect(result.current.candidates).toEqual([]);
    expect(result.current.loading).toBe(false);
  });

  it("读取配置失败时不请求候选，也不允许用空表覆盖现有术语", async () => {
    mockCommands({
      get_config: () => Promise.reject("config failed"),
    });
    const { result } = renderHook(() => useGlossaryCandidates(t));

    act(() => result.current.open("e1"));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe("config failed");
    expect(result.current.glossaryLoaded).toBe(false);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "suggest_glossary_terms",
      expect.anything(),
    );

    act(() => result.current.addManual("平动点"));
    act(() => result.current.add());
    expect(invokeMock).not.toHaveBeenCalledWith("save_config", expect.anything());
  });
});
