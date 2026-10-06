/** 单条历史条目：时间、文本、复制与润色操作、附令润色输入区。 */
import {
  Copy,
  Check,
  FileText,
  Sparkles,
  PenLine,
  Loader2,
} from "lucide-react";
import type { HistoryEntry } from "./useHistoryPanel";

interface HistoryEntryItemProps {
  t: (key: string) => string;
  entry: HistoryEntry;
  uiLang: string;
  selected: boolean;
  copiedText: boolean;
  copiedRaw: boolean;
  polishing: boolean;
  instructionOpen: boolean;
  instructionText: string;
  onToggle: () => void;
  onCopyText: () => void;
  onCopyRaw: () => void;
  onPolish: () => void;
  onOpenInstruction: () => void;
  onCloseInstruction: () => void;
  onInstructionChange: (value: string) => void;
  onInstructionSubmit: () => void;
}

function formatTime(ms: number, locale: string): string {
  const loc = locale === "en" ? "en-US" : "zh-CN";
  return new Date(ms).toLocaleString(loc, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function HistoryEntryItem({
  t, entry, uiLang, selected, copiedText, copiedRaw, polishing,
  instructionOpen, instructionText, onToggle, onCopyText, onCopyRaw, onPolish,
  onOpenInstruction, onCloseInstruction, onInstructionChange, onInstructionSubmit,
}: HistoryEntryItemProps) {
  const e = entry;

  return (
    <li className="history-item">
      <label className="history-item-check">
        <input
          type="checkbox"
          checked={selected}
          onChange={onToggle}
        />
      </label>
      <div className="history-item-body">
        <time className="history-item-time" dateTime={new Date(e.createdAtMs).toISOString()}>
          {formatTime(e.createdAtMs, uiLang)}
        </time>
        <p className="history-item-text">{e.text}</p>
        {e.rawText !== e.text && (
          <p className="history-item-raw">
            <span className="history-item-raw-label">{t("history.raw_label")}</span>
            {e.rawText}
          </p>
        )}
        <HistoryEntryActions
          t={t} copiedText={copiedText} copiedRaw={copiedRaw} polishing={polishing}
          onCopyText={onCopyText} onCopyRaw={onCopyRaw} onPolish={onPolish}
          onOpenInstruction={onOpenInstruction}
        />
        {instructionOpen && (
          <HistoryInstructionArea
            t={t} polishing={polishing} instructionText={instructionText}
            onInstructionChange={onInstructionChange}
            onInstructionSubmit={onInstructionSubmit} onCloseInstruction={onCloseInstruction}
          />
        )}
      </div>
    </li>
  );
}

/** 操作行：复制润色文本、复制原始转写、直接润色与附令润色。 */
function HistoryEntryActions({
  t, copiedText, copiedRaw, polishing,
  onCopyText, onCopyRaw, onPolish, onOpenInstruction,
}: Pick<
  HistoryEntryItemProps,
  "t" | "copiedText" | "copiedRaw" | "polishing" | "onCopyText" | "onCopyRaw"
  | "onPolish" | "onOpenInstruction"
>) {
  return (
    <div className="history-item-actions">
      <CopyTextButton t={t} copiedText={copiedText} onCopyText={onCopyText} />
      <CopyRawButton t={t} copiedRaw={copiedRaw} onCopyRaw={onCopyRaw} />
      <PolishButton t={t} polishing={polishing} onPolish={onPolish} />
      <PolishWithInstructionButton
        t={t} polishing={polishing} onOpenInstruction={onOpenInstruction}
      />
    </div>
  );
}

function CopyTextButton({
  t, copiedText, onCopyText,
}: Pick<HistoryEntryItemProps, "t" | "copiedText" | "onCopyText">) {
  return (
    <button
      type="button"
      className={`history-btn history-btn--small ${copiedText ? "history-btn--copied" : ""}`}
      onClick={onCopyText}
      title={t("history.copy")}
    >
      {copiedText ? (
        <>
          <Check size={14} />
          {t("history.copied")}
        </>
      ) : (
        <>
          <Copy size={14} />
          {t("history.copy")}
        </>
      )}
    </button>
  );
}

function CopyRawButton({
  t, copiedRaw, onCopyRaw,
}: Pick<HistoryEntryItemProps, "t" | "copiedRaw" | "onCopyRaw">) {
  return (
    <button
      type="button"
      className={`history-btn history-btn--small ${copiedRaw ? "history-btn--copied" : ""}`}
      onClick={onCopyRaw}
      title={t("history.copy_raw")}
    >
      {copiedRaw ? (
        <>
          <Check size={14} />
          {t("history.raw_copied")}
        </>
      ) : (
        <>
          <FileText size={14} />
          {t("history.copy_raw")}
        </>
      )}
    </button>
  );
}

function PolishButton({
  t, polishing, onPolish,
}: Pick<HistoryEntryItemProps, "t" | "polishing" | "onPolish">) {
  return (
    <button
      type="button"
      className="history-btn history-btn--small history-btn--accent"
      disabled={polishing}
      onClick={onPolish}
      title={t("history.polish")}
    >
      {polishing ? (
        <Loader2 size={14} className="history-spin" />
      ) : (
        <Sparkles size={14} />
      )}
      {t("history.polish")}
    </button>
  );
}

function PolishWithInstructionButton({
  t, polishing, onOpenInstruction,
}: Pick<HistoryEntryItemProps, "t" | "polishing" | "onOpenInstruction">) {
  return (
    <button
      type="button"
      className="history-btn history-btn--small"
      disabled={polishing}
      onClick={onOpenInstruction}
      title={t("history.polish_with_instruction")}
    >
      <PenLine size={14} />
      {t("history.polish_with_instruction")}
    </button>
  );
}

/** 附令润色输入区：输入自定义指令后提交润色。 */
function HistoryInstructionArea({
  t, polishing, instructionText, onInstructionChange,
  onInstructionSubmit, onCloseInstruction,
}: Pick<
  HistoryEntryItemProps,
  "t" | "polishing" | "instructionText" | "onInstructionChange"
  | "onInstructionSubmit" | "onCloseInstruction"
>) {
  return (
    <div className="history-item-instruction">
      <input
        type="text"
        className="history-instruction-input"
        value={instructionText}
        onChange={(ev) => onInstructionChange(ev.target.value)}
        onKeyDown={(ev) => {
          if (ev.key === "Enter" && !polishing) {
            onInstructionSubmit();
          }
        }}
        disabled={polishing}
        placeholder={t("history.instruction_placeholder")}
      />
      <button
        type="button"
        className="history-btn history-btn--small history-btn--accent"
        disabled={polishing}
        onClick={onInstructionSubmit}
      >
        {polishing ? (
          <Loader2 size={14} className="history-spin" />
        ) : (
          t("history.submit")
        )}
      </button>
      <button
        type="button"
        className="history-btn history-btn--small"
        disabled={polishing}
        onClick={onCloseInstruction}
      >
        {t("history.cancel")}
      </button>
    </div>
  );
}
