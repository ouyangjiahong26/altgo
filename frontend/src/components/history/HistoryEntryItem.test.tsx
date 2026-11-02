/** 历史条目的来源与耗时展示：徽标、各环节耗时格式、旧条目无元数据时的回退。 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HistoryEntryItem } from "./HistoryEntryItem";
import type { HistoryEntry } from "./useHistoryPanel";
import type { GlossaryCandidateController } from "./useGlossaryCandidates";

function makeCandidate(
  overrides: Partial<GlossaryCandidateController> = {},
): GlossaryCandidateController {
  return {
    openId: null,
    loading: false,
    remoteReady: true,
    glossaryLoaded: true,
    error: null,
    candidates: [],
    selected: new Set(),
    manualTerms: [],
    manualInput: "",
    manualError: "",
    adding: false,
    addedCount: 0,
    skipped: [],
    glossaryCount: 0,
    open: vi.fn(),
    close: vi.fn(),
    toggle: vi.fn(),
    setManualInput: vi.fn(),
    addManual: vi.fn(),
    removeManual: vi.fn(),
    add: vi.fn(),
    ...overrides,
  };
}

const t = (key: string): string => key;

function makeEntry(meta?: HistoryEntry["meta"]): HistoryEntry {
  return {
    id: "e1",
    createdAtMs: Date.parse("2026-01-01T08:00:00Z"),
    rawText: "原文",
    text: "润色后",
    meta: meta ?? null,
  };
}

function renderEntry(
  entry: HistoryEntry,
  uiLang = "zh",
  candidate: GlossaryCandidateController = makeCandidate(),
) {
  return render(
    <HistoryEntryItem
      t={t}
      entry={entry}
      uiLang={uiLang}
      candidate={candidate}
      selected={false}
      copiedText={false}
      copiedRaw={false}
      polishing={false}
      instructionOpen={false}
      instructionText=""
      onToggle={() => {}}
      onCopyText={() => {}}
      onCopyRaw={() => {}}
      onPolish={() => {}}
      onOpenInstruction={() => {}}
      onCloseInstruction={() => {}}
      onInstructionChange={() => {}}
      onInstructionSubmit={() => {}}
    />,
  );
}

describe("HistoryEntryItem 来源与耗时", () => {
  it("有 meta 时展示本地徽标与三个环节耗时", () => {
    renderEntry(
      makeEntry({ backend: "local", recordingMs: 3200, transcribeMs: 850, polishMs: 1200 }),
    );
    expect(screen.getByText("history.backend_local")).toBeTruthy();
    expect(screen.getByText("history.stage_recording 3.2 秒")).toBeTruthy();
    expect(screen.getByText("history.stage_transcribe 850 毫秒")).toBeTruthy();
    expect(screen.getByText("history.stage_polish 1.2 秒")).toBeTruthy();
  });

  it("在线后端用在线徽标，未启用润色时不展示润色耗时", () => {
    renderEntry(
      makeEntry({ backend: "online", recordingMs: 1000, transcribeMs: 2400, polishMs: null }),
    );
    expect(screen.getByText("history.backend_online")).toBeTruthy();
    expect(screen.queryByText(/history.stage_polish/)).toBeNull();
  });

  it("超过 1 分钟的耗时换算为分秒", () => {
    renderEntry(
      makeEntry({ backend: "local", recordingMs: 63000, transcribeMs: 0, polishMs: null }),
    );
    expect(screen.getByText("history.stage_recording 1 分 3 秒")).toBeTruthy();
  });

  it("取整进位不产生 60 秒", () => {
    // 119.6 秒应显示 2 分 0 秒而不是 1 分 60 秒；59.95 秒同理进位到 1 分 0 秒。
    renderEntry(
      makeEntry({ backend: "local", recordingMs: 119600, transcribeMs: 0, polishMs: null }),
    );
    expect(screen.getByText("history.stage_recording 2 分 0 秒")).toBeTruthy();

    renderEntry(
      makeEntry({ backend: "local", recordingMs: 59950, transcribeMs: 0, polishMs: null }),
    );
    expect(screen.getByText("history.stage_recording 1 分 0 秒")).toBeTruthy();
  });

  it("英文界面用英文单位", () => {
    renderEntry(
      makeEntry({ backend: "local", recordingMs: 3200, transcribeMs: 850, polishMs: null }),
      "en",
    );
    expect(screen.getByText("history.stage_recording 3.2 s")).toBeTruthy();
    expect(screen.getByText("history.stage_transcribe 850 ms")).toBeTruthy();
  });

  it("旧条目无 meta 时不渲染来源与耗时行", () => {
    const { container } = renderEntry(makeEntry());
    expect(container.querySelector(".history-item-meta")).toBeNull();
  });
});

describe("HistoryEntryItem 候选术语入口", () => {
  it("含汉字的条目显示检查术语按钮", () => {
    renderEntry(makeEntry());

    expect(
      screen.getByRole("button", { name: "history.glossary_candidates_open" }),
    ).toBeTruthy();
  });

  it("纯英文条目不显示检查术语按钮", () => {
    renderEntry({ ...makeEntry(), rawText: "hello", text: "hello" });

    expect(
      screen.queryByRole("button", {
        name: "history.glossary_candidates_open",
      }),
    ).toBeNull();
  });

  it("点击检查术语打开对应条目", () => {
    const candidate = makeCandidate();
    renderEntry(makeEntry(), "zh", candidate);

    fireEvent.click(
      screen.getByRole("button", {
        name: "history.glossary_candidates_open",
      }),
    );

    expect(candidate.open).toHaveBeenCalledWith("e1");
  });

  it("候选面板打开时渲染候选", () => {
    renderEntry(
      makeEntry(),
      "zh",
      makeCandidate({
        openId: "e1",
        candidates: [{ term: "地月空间", fragment: "地约空间" }],
      }),
    );

    expect(screen.getByText("地月空间")).toBeTruthy();
    expect(screen.getByText(/地约空间/)).toBeTruthy();
  });
});
