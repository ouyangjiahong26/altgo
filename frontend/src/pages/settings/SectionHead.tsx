import { ChevronDown, ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

/**
 * 取区块说明文案。字典里缺条目时 t 原样返回 key，说明属于补充信息，
 * 缺条目就不渲染，绝不把内部 key 当文案显示给用户。
 */
export function sectionNote(
  t: (key: string) => string,
  key: string,
): string | undefined {
  const text = t(key);
  return text === key ? undefined : text;
}

interface SectionHeadProps {
  /** 区块标记：14px 线性图标，或 20px 以内的品牌小图。 */
  leading: ReactNode;
  title: string;
  note?: string;
}

/** 区块头：小标题加一句话说明，站在内容卡上方，不占用卡内空间。 */
export function SectionHead({ leading, title, note }: SectionHeadProps) {
  return (
    <header className="settings-section-head">
      <h3 className="settings-section-head-title">
        {leading}
        {title}
      </h3>
      {note && <p className="settings-section-head-note">{note}</p>}
    </header>
  );
}

interface CollapsibleSectionHeadProps extends SectionHeadProps {
  expanded: boolean;
  onToggle: () => void;
  /** 折叠内容区的 id，按钮用 aria-controls 指过去。 */
  controls: string;
}

/**
 * 可折叠区块头：整条是按钮，开合状态用 aria-expanded 表达。
 * 外面套 h3，保证与其他区块一样出现在标题大纲里。
 * 可访问名称固定为标题本身，说明句只作为可见文字，不把名称撑长。
 */
export function CollapsibleSectionHead({
  leading,
  title,
  note,
  expanded,
  onToggle,
  controls,
}: CollapsibleSectionHeadProps) {
  return (
    <h3 className="settings-section-head-group">
      <button
        type="button"
        className="settings-section-head settings-section-head--toggle"
        aria-expanded={expanded}
        aria-controls={controls}
        aria-label={title}
        onClick={onToggle}
      >
        <span className="settings-section-head-title">
          {leading}
          {title}
          {expanded ? (
            <ChevronDown size={15} className="settings-section-head-chevron" aria-hidden="true" />
          ) : (
            <ChevronRight size={15} className="settings-section-head-chevron" aria-hidden="true" />
          )}
        </span>
        {note && <span className="settings-section-head-note">{note}</span>}
      </button>
    </h3>
  );
}
