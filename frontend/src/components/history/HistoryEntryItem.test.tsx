/** 历史条目的来源与耗时展示：徽标、各环节耗时格式、旧条目无元数据时的回退。 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HistoryEntryItem } from "./HistoryEntryItem";
import type { HistoryEntry } from "./useHistoryPanel";

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

function renderEntry(entry: HistoryEntry, uiLang = "zh") {
  return render(
    <HistoryEntryItem
      t={t}
      entry={entry}
      uiLang={uiLang}
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
