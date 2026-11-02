import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { HistoryEntry } from "./useHistoryPanel";
import type { GlossaryCandidateController } from "./useGlossaryCandidates";
import { GlossaryCandidatePanel } from "./GlossaryCandidatePanel";

const entry: HistoryEntry = {
  id: "e1",
  createdAtMs: 0,
  rawText: "地约空间",
  text: "地月空间",
};

function makeCandidate(
  overrides: Partial<GlossaryCandidateController> = {},
): GlossaryCandidateController {
  return {
    openId: "e1",
    loading: false,
    remoteReady: true,
    glossaryLoaded: true,
    error: null,
    candidates: [{ term: "地月空间", fragment: "地约空间" }],
    selected: new Set<string>(),
    manualTerms: [],
    manualInput: "",
    manualError: "",
    adding: false,
    addedCount: 0,
    skipped: [],
    glossaryCount: 1,
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

function renderPanel(
  overrides: Partial<GlossaryCandidateController> = {},
) {
  const candidate = makeCandidate(overrides);
  render(
    <GlossaryCandidatePanel t={(key) => key} entry={entry} candidate={candidate} />,
  );
  return candidate;
}

describe("GlossaryCandidatePanel", () => {
  it("展示发送提示、候选标准写法与原文片段", () => {
    renderPanel();

    expect(
      screen.getByText("history.glossary_candidates_send_note_both"),
    ).toBeTruthy();
    expect(screen.getByText("地月空间")).toBeTruthy();
    expect(
      screen.getByText(/history\.glossary_candidates_fragment/),
    ).toBeTruthy();
    expect(screen.getByText(/地约空间/)).toBeTruthy();
  });

  it("勾选候选、输入手动术语与点击加入分别触发回调", () => {
    const candidate = renderPanel({
      selected: new Set(["地月空间"]),
    });

    fireEvent.click(screen.getByRole("checkbox"));
    expect(candidate.toggle).toHaveBeenCalledWith("地月空间");

    fireEvent.change(
      screen.getByPlaceholderText("history.glossary_candidates_manual_placeholder"),
      { target: { value: "平动点" } },
    );
    expect(candidate.setManualInput).toHaveBeenCalledWith("平动点");

    fireEvent.click(
      screen.getByRole("button", { name: "history.glossary_candidates_add" }),
    );
    expect(candidate.add).toHaveBeenCalled();
  });

  it("本地后端显示提示，手动输入仍可用", () => {
    renderPanel({
      remoteReady: false,
      candidates: [],
      manualTerms: ["平动点"],
    });

    expect(
      screen.getByText("history.glossary_candidates_remote_missing"),
    ).toBeTruthy();
    expect(
      screen.getByPlaceholderText("history.glossary_candidates_manual_placeholder"),
    ).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "history.glossary_candidates_add",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });

  it("没有候选时显示空提示", () => {
    renderPanel({ candidates: [] });

    expect(
      screen.getByText("history.glossary_candidates_empty"),
    ).toBeTruthy();
  });

  it("加入候选后不再显示空候选提示", () => {
    renderPanel({ candidates: [], addedCount: 1 });

    expect(
      screen.queryByText("history.glossary_candidates_empty"),
    ).toBeNull();
    expect(
      screen.getByText("history.glossary_candidates_added"),
    ).toBeTruthy();
  });

  it("展示未加入的词与原因", () => {
    renderPanel({
      skipped: [{ term: "地月空间", reason: "duplicate" }],
    });

    const skipped = screen
      .getByText(/history\.glossary_candidates_skip_duplicate/)
      .closest("li");
    expect(skipped?.textContent).toContain("地月空间");
  });

  it("配置读取失败时禁用加入按钮", () => {
    renderPanel({ glossaryLoaded: false, manualTerms: ["平动点"] });

    expect(
      (
        screen.getByRole("button", {
          name: "history.glossary_candidates_add",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });

  it("手动输入回车添加，粘贴多行直接交给上层", () => {
    const candidate = renderPanel();
    const input = screen.getByPlaceholderText(
      "history.glossary_candidates_manual_placeholder",
    );

    fireEvent.keyDown(input, { key: "Enter" });
    expect(candidate.addManual).toHaveBeenCalled();

    fireEvent.paste(input, {
      clipboardData: { getData: () => "地月空间\n平动点" },
    });
    expect(candidate.addManual).toHaveBeenCalledWith("地月空间\n平动点");
  });

  it("移除手动术语触发回调", () => {
    const candidate = renderPanel({ manualTerms: ["平动点"] });

    fireEvent.click(
      screen.getByLabelText("settings.polish_glossary_remove 平动点"),
    );
    expect(candidate.removeManual).toHaveBeenCalledWith("平动点");
  });

  it("保存期间禁用候选与手动输入", () => {
    renderPanel({
      adding: true,
      selected: new Set(["地月空间"]),
      manualTerms: ["平动点"],
    });

    expect((screen.getByRole("checkbox") as HTMLInputElement).disabled).toBe(
      true,
    );
    expect(
      (
        screen.getByPlaceholderText(
          "history.glossary_candidates_manual_placeholder",
        ) as HTMLInputElement
      ).disabled,
    ).toBe(true);
    expect(
      (
        screen.getByRole("button", {
          name: "history.glossary_candidates_add",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      (
        screen.getByRole("button", {
          name: "history.glossary_candidates_close",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });
});
