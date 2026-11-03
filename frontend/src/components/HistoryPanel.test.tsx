/** 历史列表的分块渲染：首屏条目数有上限，哨兵进入视口后补挂下一块，
 * 全选只作用于已渲染条目。这是主页重挂载耗时的回归锁。 */
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import HistoryPanel from "./HistoryPanel";
import type { HistoryEntry } from "./history/useHistoryPanel";

const { invokeMock, ioInstances } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  ioInstances: [] as Array<{
    callback: (entries: Array<{ isIntersecting: boolean }>) => void;
  }>,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

vi.mock("../i18n", () => ({
  resolveUiLang: () => "zh",
  useTranslation: () => ({ t: (key: string) => key, lang: "zh", setLang: vi.fn() }),
}));

// jsdom 没有 IntersectionObserver，这里记录实例让测试手动触发补挂。
class IntersectionObserverStub {
  callback: (entries: Array<{ isIntersecting: boolean }>) => void;
  constructor(callback: (entries: Array<{ isIntersecting: boolean }>) => void) {
    this.callback = callback;
    ioInstances.push(this);
  }
  observe = vi.fn();
  disconnect = vi.fn();
}

function makeEntries(n: number): HistoryEntry[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `e${i}`,
    createdAtMs: Date.parse("2026-01-01T08:00:00Z") + i,
    rawText: `原文${i}`,
    text: `文本${i}`,
    meta: null,
  }));
}

function itemNodes(): NodeListOf<HTMLElement> {
  return document.querySelectorAll(".history-list > li");
}

describe("历史列表分块渲染", () => {
  beforeEach(() => {
    vi.stubGlobal("IntersectionObserver", IntersectionObserverStub);
    ioInstances.length = 0;
    invokeMock.mockReset();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history") return Promise.resolve(makeEntries(250));
      return Promise.resolve(null);
    });
  });

  it("250 条历史首屏只挂 100 条，滚动哨兵补挂下一块", async () => {
    render(<HistoryPanel />);

    await waitFor(() => expect(itemNodes().length).toBe(100));
    expect(document.querySelector(".history-more-sentinel")).not.toBeNull();

    const io = ioInstances[ioInstances.length - 1];
    io.callback([{ isIntersecting: true }]);

    await waitFor(() => expect(itemNodes().length).toBe(200));
    expect(document.querySelector(".history-more-sentinel")).not.toBeNull();
  });

  it("全部补挂完成后哨兵消失", async () => {
    render(<HistoryPanel />);

    await waitFor(() => expect(itemNodes().length).toBe(100));
    ioInstances[ioInstances.length - 1].callback([{ isIntersecting: true }]);
    await waitFor(() => expect(itemNodes().length).toBe(200));
    ioInstances[ioInstances.length - 1].callback([{ isIntersecting: true }]);

    await waitFor(() => {
      expect(itemNodes().length).toBe(250);
      expect(document.querySelector(".history-more-sentinel")).toBeNull();
    });
  });

  it("全选只选中已渲染的条目，删除所选只提交这些 id", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<HistoryPanel />);

    await waitFor(() => expect(itemNodes().length).toBe(100));
    const selectAll = screen.getByLabelText("history.select_all");
    selectAll.click();
    screen.getByText("history.delete_selected").click();

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("delete_history_entries", {
      ids: expect.arrayContaining(["e0", "e99"]),
    }));
    const call = invokeMock.mock.calls.find(([cmd]) => cmd === "delete_history_entries");
    expect(call?.[1].ids).toHaveLength(100);
    expect(confirmSpy).toHaveBeenCalled();
  });
});
