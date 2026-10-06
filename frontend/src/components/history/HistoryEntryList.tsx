/** 历史条目列表：把面板状态映射为各条目的选中、复制与润色回调。 */
import type { useHistoryPanel } from "./useHistoryPanel";
import { HistoryEntryItem } from "./HistoryEntryItem";

type Panel = ReturnType<typeof useHistoryPanel>;

interface HistoryEntryListProps {
  t: (key: string) => string;
  uiLang: string;
  h: Panel;
}

export function HistoryEntryList({ t, uiLang, h }: HistoryEntryListProps) {
  return (
    <ul className="history-list">
      {h.entries.map((e) => (
        <HistoryEntryItem
          key={e.id}
          t={t}
          entry={e}
          uiLang={uiLang}
          selected={h.selected.has(e.id)}
          copiedText={h.copiedId === `${e.id}:text`}
          copiedRaw={h.copiedId === `${e.id}:raw`}
          polishing={h.polishingId === e.id}
          instructionOpen={h.instructionId === e.id}
          instructionText={h.instructionText}
          onToggle={() => h.toggleOne(e.id)}
          onCopyText={() => void h.handleCopy(`${e.id}:text`, e.text)}
          onCopyRaw={() => void h.handleCopy(`${e.id}:raw`, e.rawText)}
          onPolish={() => void h.handlePolish(e.id)}
          onOpenInstruction={() => h.openInstruction(e.id)}
          onCloseInstruction={h.closeInstruction}
          onInstructionChange={h.setInstructionText}
          onInstructionSubmit={() => void h.handlePolish(e.id, h.instructionText)}
        />
      ))}
    </ul>
  );
}
