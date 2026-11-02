/** 术语标签编辑器：设置页与历史候选面板共用的标签、输入与键盘交互。 */
import { useRef } from "react";
import { GLOSSARY_SEPARATORS } from "../utils/glossary";

interface GlossaryChipEditorProps {
  t: (key: string) => string;
  terms: string[];
  input: string;
  placeholder: string;
  disabled?: boolean;
  onInputChange: (value: string) => void;
  onAdd: (text?: string) => void;
  onRemove: (term: string) => void;
}

export function GlossaryChipEditor({
  t,
  terms,
  input,
  placeholder,
  disabled = false,
  onInputChange,
  onAdd,
  onRemove,
}: GlossaryChipEditorProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  return (
    <div
      className="glossary-editor"
      onClick={() => inputRef.current?.focus()}
    >
      {terms.map((term) => (
        <span className="glossary-chip" key={term}>
          <span>{term}</span>
          <button
            type="button"
            className="glossary-chip-remove"
            aria-label={`${t("settings.polish_glossary_remove")} ${term}`}
            disabled={disabled}
            onClick={() => onRemove(term)}
          >
            ×
          </button>
        </span>
      ))}
      <input
        ref={inputRef}
        className="glossary-input"
        type="text"
        value={input}
        placeholder={placeholder}
        aria-label={placeholder}
        disabled={disabled}
        onChange={(event) => onInputChange(event.target.value)}
        onKeyDown={(event) => {
          if (
            event.key === "Enter" ||
            event.key === "," ||
            event.key === "，"
          ) {
            event.preventDefault();
            onAdd();
          } else if (
            event.key === "Backspace" &&
            !input &&
            terms.length > 0
          ) {
            onRemove(terms[terms.length - 1]);
          }
        }}
        onPaste={(event) => {
          const text = event.clipboardData.getData("text");
          if (!GLOSSARY_SEPARATORS.test(text)) return;
          event.preventDefault();
          onAdd(text);
        }}
      />
    </div>
  );
}
